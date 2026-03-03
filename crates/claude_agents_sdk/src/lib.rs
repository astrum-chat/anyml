mod transport;
pub mod types;

use std::path::PathBuf;

use futures::Stream;
use thiserror::Error;

pub use transport::AgentHandle;
pub use types::{AgentMessage, ContentBlock, Message, QueryOptions, Role};

/// Errors that can occur when using the Claude Agent SDK.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Claude CLI not found on PATH")]
    CliNotFound(#[source] anyhow::Error),

    #[error("Failed to spawn Claude CLI process")]
    SpawnFailed(#[source] anyhow::Error),

    #[error("I/O error reading from CLI")]
    Io(#[source] anyhow::Error),

    #[error("No user message found in conversation history")]
    NoUserMessage,
}

/// Client for the Claude Code CLI.
///
/// Spawns the CLI as a subprocess and streams back NDJSON messages.
/// Async-runtime-agnostic — uses `std::thread` + `futures::channel::mpsc`.
pub struct ClaudeAgentSDK {
    cli_path: Option<PathBuf>,
}

impl ClaudeAgentSDK {
    /// Create a new SDK instance.
    ///
    /// By default, the `claude` binary is resolved from `PATH` at query time.
    /// Use [`.cli_path()`] to override.
    pub fn new() -> Self {
        Self { cli_path: None }
    }

    /// Set a custom path to the Claude CLI binary.
    pub fn cli_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.cli_path = Some(path.into());
        self
    }

    /// Run a query with a message history.
    ///
    /// Returns a stream of [`AgentMessage`]s and a handle to the subprocess.
    /// The last user message in the history is used as the prompt.
    /// All tools are disabled.
    pub fn query(
        &self,
        messages: &[Message],
        options: &QueryOptions,
    ) -> Result<(impl Stream<Item = Result<AgentMessage, AgentError>>, AgentHandle), AgentError>
    {
        let cli = self.resolve_cli()?;
        transport::spawn_agent(cli, messages, options)
    }

    fn resolve_cli(&self) -> Result<PathBuf, AgentError> {
        match &self.cli_path {
            Some(path) => Ok(path.clone()),
            None => which::which("claude").map_err(|e| AgentError::CliNotFound(anyhow::anyhow!(e))),
        }
    }
}

impl Default for ClaudeAgentSDK {
    fn default() -> Self {
        Self::new()
    }
}
