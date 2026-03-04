use std::path::PathBuf;

mod chat;
mod list_models;

pub use claude_sdk::install_cli;

pub struct ClaudeSdkProvider {
    sdk: claude_sdk::ClaudeSDK,
}

impl ClaudeSdkProvider {
    pub fn new(cli_path: impl Into<PathBuf>) -> Self {
        Self {
            sdk: claude_sdk::ClaudeSDK::new(cli_path),
        }
    }

    pub fn api_key(mut self, key: secrecy::SecretString) -> Self {
        self.sdk = self.sdk.api_key(key);
        self
    }
}
