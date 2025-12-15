use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::{collections::HashMap, pin::Pin, usize};
use thiserror::Error;

use crate::Message;

pub type ChatResponse<'a> =
    Pin<Box<dyn Stream<Item = Result<ChunkResponse, ChatStreamError>> + Send + 'a>>;

#[async_trait::async_trait]
pub trait ChatProvider: Send + Sync {
    async fn chat(&self, options: &ChatOptions<'_>) -> Result<ChatResponse, ChatError>;
}

#[derive(Serialize, Clone, Debug)]
pub struct ChatOptions<'a> {
    model: &'a str,
    messages: Messages<'a>,
    stream: bool,
    max_tokens: usize,
    #[serde(flatten)]
    extras: HashMap<String, serde_json::Value>,
}

impl<'a> ChatOptions<'a> {
    pub fn new(model: &'a str) -> Self {
        Self {
            model,
            messages: Messages::Raw(&[]),
            stream: true,
            max_tokens: 4096,
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
        self.messages = Messages::Raw(messages);
        self
    }

    /// Sets the messages in an already-serialized format to be used for the chat query.
    /// It's up to the consumer to ensure the serialized messages are valid.
    pub fn messages_serialized(mut self, messages: Box<RawValue>) -> Self {
        self.messages = Messages::Serialized(messages);
        self
    }

    /// Enables or disables streaming mode.
    /// If `false` then the entire response will be returned in one chunk.
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub fn max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens.max(1);
        self
    }

    /// Adds an extra non-standard field that is bespoke to a specific provider.
    pub fn extra(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.extras.insert(key.into(), value.into());
        self
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
enum Messages<'a> {
    Raw(&'a [Message]),
    Serialized(Box<RawValue>),
}

#[derive(Deserialize, Debug)]
pub struct ChunkResponse {
    pub content: String,
}

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("Failed to build the request: {0}.")]
    RequestBuildFailed(#[source] anyhow::Error),

    #[error("Failed to retrieve the response: {0}.")]
    ResponseFetchFailed(#[source] anyhow::Error),

    #[error("The request failed: {0}.")]
    RequestError(#[source] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum ChatStreamError {
    #[error("This chunk contains incomplete data.")]
    IncompleteChunk,

    #[error("Failed to parse chunk: {0}.")]
    ParseError(#[source] anyhow::Error),
}
