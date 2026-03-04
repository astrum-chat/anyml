use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use futures::channel::mpsc;
use futures::Stream;
use secrecy::{ExposeSecret, SecretString};

use crate::types::{AgentMessage, Message, QueryOptions, Role, ThinkingConfig};
use crate::AgentError;

/// Handle to the Claude CLI subprocess.
///
/// Kills the child process on drop to prevent orphaned processes.
pub struct AgentHandle {
    child: Child,
}

impl AgentHandle {
    /// Wait for the subprocess to exit and return its status.
    pub fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child.wait()
    }

    /// Kill the subprocess.
    pub fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }

    /// Check if the subprocess has exited without blocking.
    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }
}

impl Drop for AgentHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawns the Claude CLI and returns a `Stream` of parsed messages plus a
/// handle to the subprocess.
///
/// The blocking stdout reads run on a dedicated `std::thread`, bridged into
/// the async world via `futures::channel::mpsc`. This keeps the crate
/// async-runtime-agnostic.
pub fn spawn_agent(
    cli_path: &Path,
    messages: &[Message],
    options: &QueryOptions,
    api_key: Option<&SecretString>,
) -> Result<(impl Stream<Item = Result<AgentMessage, AgentError>> + use<>, AgentHandle), AgentError>
{
    if !messages.iter().any(|m| m.role == Role::User) {
        return Err(AgentError::NoUserMessage);
    }

    let mut cmd = Command::new(cli_path);

    // Run in simple mode — disables CLAUDE.md, MCP tools, hooks, and project
    // context so the CLI behaves as a pure LLM proxy.
    cmd.env("CLAUDE_CODE_SIMPLE", "1");

    // Clear inherited auth env vars so the CLI doesn't pick up stale/empty values.
    cmd.env_remove("ANTHROPIC_API_KEY");
    cmd.env_remove("CLAUDE_CODE_OAUTH_TOKEN");

    if let Some(key) = api_key {
        let secret = key.expose_secret();
        if secret.starts_with("sk-ant-api") {
            cmd.env("ANTHROPIC_API_KEY", secret);
        } else {
            cmd.env("CLAUDE_CODE_OAUTH_TOKEN", secret);
        }
    }

    cmd.arg("--print")
        .arg("--verbose")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--input-format")
        .arg("stream-json")
        .arg("--include-partial-messages")
        .arg("--setting-sources")
        .arg("")
        .arg("--tools")
        .arg("");

    // We manage conversation history ourselves (via temp .jsonl files),
    // so always disable the CLI's own session persistence.
    cmd.arg("--no-session-persistence");

    if let Some(model) = &options.model {
        cmd.arg("--model").arg(model);
    }
    if let Some(turns) = options.max_turns {
        cmd.arg("--max-turns").arg(turns.to_string());
    }

    // Find the last user message — this is what we send via stdin.
    let last_user_idx = messages
        .iter()
        .rposition(|m| m.role == Role::User)
        .expect("checked above");

    if let Some(system_prompt) = &options.system_prompt {
        cmd.arg("--system-prompt").arg(system_prompt);
    }

    // Resume an existing session so the CLI loads prior conversation turns
    // from its session file on disk.
    if let Some(session_id) = &options.session_id {
        cmd.arg("--resume").arg(session_id);
    }
    if let Some(cwd) = &options.cwd {
        cmd.current_dir(cwd);
    }

    match &options.thinking {
        Some(ThinkingConfig::BudgetTokens(budget)) => {
            cmd.env("MAX_THINKING_TOKENS", budget.to_string());
            cmd.arg("--settings")
                .arg(r#"{"alwaysThinkingEnabled":true}"#);
        }
        Some(ThinkingConfig::Effort(level)) => {
            let budget = match level.as_str() {
                "low" => 4096,
                "medium" => 16384,
                "high" => 32768,
                "max" => 128000,
                _ => 16384,
            };
            cmd.env("MAX_THINKING_TOKENS", budget.to_string());
            cmd.arg("--settings")
                .arg(r#"{"alwaysThinkingEnabled":true}"#);
        }
        Some(ThinkingConfig::Disabled) | None => {
            cmd.env("MAX_THINKING_TOKENS", "0");
        }
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| AgentError::SpawnFailed(anyhow::anyhow!(e)))?;

    // Write only the last user message to stdin as NDJSON.
    // Prior turns are managed via session persistence (--resume).
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AgentError::SpawnFailed(anyhow::anyhow!("failed to capture stdin")))?;

        let last_user_msg = &messages[last_user_idx];
        let session_id = options.session_id.as_deref().unwrap_or("");
        let stdin_msg = serde_json::json!({
            "type": "user",
            "session_id": session_id,
            "message": {
                "role": "user",
                "content": last_user_msg.content,
            },
            "parent_tool_use_id": null,
        });
        serde_json::to_writer(&mut stdin, &stdin_msg)
            .map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;
        stdin
            .write_all(b"\n")
            .map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;
        // stdin is dropped here, closing the pipe and signaling EOF to the CLI.
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AgentError::SpawnFailed(anyhow::anyhow!("failed to capture stdout")))?;

    let stderr = child.stderr.take();

    let (tx, rx) = mpsc::unbounded();

    // Read stderr concurrently on its own thread so errors are surfaced
    // immediately rather than waiting for stdout to close.
    let stderr_tx = tx.clone();
    if let Some(stderr) = stderr {
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(line) if line.trim().is_empty() => continue,
                    Ok(line) => {
                        let _ = stderr_tx.unbounded_send(Err(AgentError::Io(
                            anyhow::anyhow!("CLI stderr: {line}"),
                        )));
                    }
                    Err(_) => break,
                }
            }
        });
    }

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) if line.trim().is_empty() => continue,
                Ok(line) => {
                    let msg = serde_json::from_str::<AgentMessage>(&line);
                    match msg {
                        Ok(msg) => {
                            if tx.unbounded_send(Ok(msg)).is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            // Check if it's valid JSON we just don't model.
                            // If it has an "error" field or "type":"error", surface it.
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                                // Surface JSON error messages from the CLI
                                if val.get("error").is_some()
                                    || val.get("type").and_then(|t| t.as_str()) == Some("error")
                                {
                                    let _ = tx.unbounded_send(Err(AgentError::Io(
                                        anyhow::anyhow!("CLI error: {line}"),
                                    )));
                                }
                                // Otherwise it's a known message type we don't model
                                // (e.g. "user" tool results) — skip silently.
                            } else {
                                // Not valid JSON — raw error text from the CLI.
                                let _ = tx.unbounded_send(Err(AgentError::Io(
                                    anyhow::anyhow!("CLI: {line}"),
                                )));
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.unbounded_send(Err(AgentError::Io(anyhow::anyhow!(e))));
                    break;
                }
            }
        }
    });

    Ok((rx, AgentHandle { child }))
}


