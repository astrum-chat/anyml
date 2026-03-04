mod install;
pub mod session;
mod transport;
pub mod types;

use std::path::PathBuf;

use futures::Stream;
use secrecy::SecretString;
use thiserror::Error;

pub use install::install_cli;
pub use session::{create_session, normalize_session_id};
pub use transport::AgentHandle;
pub use types::{
    AgentMessage, ContentBlock, Message, QueryOptions, Role, StreamDelta, StreamEvent,
    ThinkingConfig,
};

/// Errors that can occur when using the Claude SDK.
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

    #[error("Failed to download Claude CLI")]
    DownloadFailed(#[source] anyhow::Error),

    #[error("Unsupported platform")]
    UnsupportedPlatform,

    #[error("Checksum verification failed")]
    ChecksumMismatch,

    #[error("Invalid file extension for CLI binary")]
    InvalidExtension,
}

/// Client for the Claude Code CLI.
///
/// Spawns the CLI as a subprocess and streams back NDJSON messages.
/// Async-runtime-agnostic — uses `std::thread` + `futures::channel::mpsc`.
pub struct ClaudeSDK {
    cli_path: PathBuf,
    api_key: Option<SecretString>,
}

impl ClaudeSDK {
    /// Create a new SDK instance with the given CLI binary path.
    pub fn new(cli_path: impl Into<PathBuf>) -> Self {
        Self {
            cli_path: cli_path.into(),
            api_key: None,
        }
    }

    /// Set the Anthropic API key. When set, this is passed to the CLI
    /// subprocess via the `ANTHROPIC_API_KEY` environment variable.
    pub fn api_key(mut self, key: SecretString) -> Self {
        self.api_key = Some(key);
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
    ) -> Result<(impl Stream<Item = Result<AgentMessage, AgentError>> + use<>, AgentHandle), AgentError>
    {
        if !self.cli_path.exists() {
            install::install_cli(&self.cli_path)?;
        }
        transport::spawn_agent(
            &self.cli_path,
            messages,
            options,
            self.api_key.as_ref(),
        )
    }
}

