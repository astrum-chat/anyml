use futures::Stream;
use std::{collections::HashMap, pin::Pin};
use thiserror::Error;

use crate::Message;

pub type ChatResponse<'a> =
    Pin<Box<dyn Stream<Item = Result<String, ChatStreamError>> + Send + 'a>>;

#[async_trait::async_trait]
pub trait ChatProvider: Send + Sync {
    async fn chat(&self, options: &ChatOptions<'_>) -> Result<ChatResponse, ChatError>;
}

pub struct ChatOptions<'a> {
    model: &'a str,
    messages: &'a [Message],
    stream: bool,
    extras: HashMap<String, serde_json::Value>,
}

impl<'a> ChatOptions<'a> {
    pub fn new(model: &'a str) -> Self {
        Self {
            model,
            messages: &[],
            stream: true,
            extras: HashMap::new(),
        }
    }

    /// Sets the model to be used for the chat query.
    pub fn model(mut self, model: &'a str) -> Self {
        self.model = model;
        self
    }

    /// Sets the messages to be used for the chat query.
    pub fn messages(mut self, messages: &'a [Message]) -> Self {
        self.messages = messages;
        self
    }

    /// Enables or disables streaming mode.
    /// If `false` then the entire response will be returned in one chunk.
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    /// Adds an extra non-standard field that is bespoke to a specific provider.
    pub fn extra(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.extras.insert(key.into(), value.into());
        self
    }

    /// Serializes to json using the specified strings for the different message role types.
    pub fn to_json(
        &self,
        user_role: &str,
        assistant_role: &str,
        system_role: &str,
        tool_role: &str,
    ) -> Result<String, serde_json::Error> {
        let mut options_json = serde_json::json!({
            "model": self.model,
            "stream": self.stream,
        });

        if let serde_json::Value::Object(ref mut map) = options_json {
            let messages_json: serde_json::Value = self
                .messages
                .iter()
                .map(|this| this.as_json(user_role, assistant_role, system_role, tool_role))
                .collect();

            map.insert("messages".to_string(), messages_json);

            for (key, value) in &self.extras {
                map.insert(key.clone(), value.clone());
            }
        }

        Ok(options_json.to_string())
    }
}

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("Failed to build the request.")]
    RequestBuildFailed(#[source] anyhow::Error),

    #[error("Failed to retrieve the response.")]
    ResponseFetchFailed(#[source] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum ChatStreamError {
    #[error("Failed to parse chunk.")]
    ParseError(#[source] anyhow::Error),
}
