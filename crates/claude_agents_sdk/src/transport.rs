use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use futures::channel::mpsc;
use futures::Stream;

use crate::types::{AgentMessage, Message, QueryOptions, Role};
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
    cli_path: PathBuf,
    messages: &[Message],
    options: &QueryOptions,
) -> Result<(impl Stream<Item = Result<AgentMessage, AgentError>>, AgentHandle), AgentError> {
    let prompt = extract_prompt(messages)?;

    let mut cmd = Command::new(cli_path);

    cmd.arg("--output-format")
        .arg("stream-json")
        .arg("--print")
        .arg("--tools")
        .arg("");

    if let Some(model) = &options.model {
        cmd.arg("--model").arg(model);
    }
    if let Some(turns) = options.max_turns {
        cmd.arg("--max-turns").arg(turns.to_string());
    }
    if let Some(system_prompt) = &options.system_prompt {
        cmd.arg("--system-prompt").arg(system_prompt);
    }
    if let Some(cwd) = &options.cwd {
        cmd.current_dir(cwd);
    }

    cmd.arg("--").arg(&prompt);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| AgentError::SpawnFailed(anyhow::anyhow!(e)))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AgentError::SpawnFailed(anyhow::anyhow!("failed to capture stdout")))?;

    let (tx, rx) = mpsc::unbounded();

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
                            // Unknown message type — skip silently.
                            // The CLI emits types we don't model (e.g. "user" tool results).
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

/// Extracts the prompt string from the message history.
///
/// Uses the content of the last user message as the prompt.
fn extract_prompt(messages: &[Message]) -> Result<String, AgentError> {
    messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.clone())
        .ok_or(AgentError::NoUserMessage)
}
