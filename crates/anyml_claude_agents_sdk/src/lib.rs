use std::path::PathBuf;

mod chat;
mod list_models;

pub use claude_agents_sdk::install_cli;

pub struct ClaudeAgentsProvider {
    sdk: claude_agents_sdk::ClaudeAgentSDK,
}

impl ClaudeAgentsProvider {
    pub fn new(cli_path: impl Into<PathBuf>) -> Self {
        Self {
            sdk: claude_agents_sdk::ClaudeAgentSDK::new(cli_path),
        }
    }

    pub fn api_key(mut self, key: secrecy::SecretString) -> Self {
        self.sdk = self.sdk.api_key(key);
        self
    }
}
