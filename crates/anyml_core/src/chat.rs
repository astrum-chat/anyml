use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    pin::Pin,
    usize,
};
use thiserror::Error;

use crate::Message;

pub struct ChatResponse<'a>(
    Pin<Box<dyn Stream<Item = Result<ChatChunk, ChatStreamError>> + Send + 'a>>,
);

impl<'a> ChatResponse<'a> {
    pub fn new(stream: impl Stream<Item = Result<ChatChunk, ChatStreamError>> + Send + 'a) -> Self {
        Self(Box::pin(stream))
    }

    pub async fn next(&mut self) -> Option<Result<ChatChunk, ChatStreamError>> {
        self.0.next().await
    }

    // Iterates through all of the remaining chunks in the stream and aggregates them into one chunk.
    // If any error occurs then it will be returned instead.
    pub async fn aggregate(&mut self) -> Option<Result<ChatChunk, ChatStreamError>> {
        let mut aggregated_chunks = match self.next().await? {
            Ok(chunk) => chunk,
            Err(err) => return Some(Err(err)),
        };

        while let Some(chunk) = self.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(err) => return Some(Err(err)),
            };

            aggregated_chunks.aggregate_with(&chunk);
        }

        Some(Ok(aggregated_chunks))
    }

    // Iterates through all of the remaining chunks in the stream and aggregates them into one chunk.
    // Any errors will be ignored.
    pub async fn aggregate_lossy(&mut self) -> Option<ChatChunk> {
        let mut aggregated_chunks = ChatChunk::default();

        while let Some(chunk) = self.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(_err) => continue,
            };

            aggregated_chunks.aggregate_with(&chunk);
        }

        Some(aggregated_chunks)
    }
}

impl<'a> Stream for ChatResponse<'a> {
    type Item = Result<ChatChunk, ChatStreamError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.0.poll_next_unpin(cx)
    }
}

impl<'a> Deref for ChatResponse<'a> {
    type Target = Pin<Box<dyn Stream<Item = Result<ChatChunk, ChatStreamError>> + Send + 'a>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for ChatResponse<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a> Unpin for ChatResponse<'a> {}

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

#[derive(Deserialize, Debug, Default)]
pub struct ChatChunk {
    pub content: String,
}

impl ChatChunk {
    pub fn aggregate_with(&mut self, chunk: &ChatChunk) {
        self.content.push_str(&chunk.content);
    }
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
