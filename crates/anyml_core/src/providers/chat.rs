use enum_kinds::EnumKind;
use futures::{Stream, StreamExt};
use serde_json::value::RawValue;
use std::{
    ops::{Deref, DerefMut},
    pin::Pin,
};
use thiserror::Error;

use crate::models::Message;

#[async_trait::async_trait]
pub trait ChatProvider: Send + Sync {
    async fn chat(&self, options: &ChatOptions<'_>) -> Result<ChatResponse, ChatError>;
}

#[derive(Clone, Debug)]
pub struct ChatOptions<'a> {
    pub model: &'a str,
    pub messages: Messages<'a>,
    pub stream: bool,
    pub max_tokens: usize,
    pub thinking: Option<Thinking>,
}

impl<'a> ChatOptions<'a> {
    pub fn new(model: &'a str) -> Self {
        Self {
            model,
            messages: Messages::Raw(&[]),
            stream: true,
            max_tokens: 4096,
            thinking: None,
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

    /// Enables thinking/reasoning for models that support it.
    pub fn thinking(mut self, thinking: Thinking) -> Self {
        self.thinking = Some(thinking);
        self
    }
}

#[derive(Clone, Debug)]
pub enum Messages<'a> {
    Raw(&'a [Message]),
    Serialized(Box<RawValue>),
}

impl Messages<'_> {
    /// Returns messages as a JSON string for embedding in request bodies.
    pub fn to_json(&self) -> String {
        match self {
            Messages::Raw(msgs) => serde_json::to_string(msgs).unwrap(),
            Messages::Serialized(raw) => raw.get().to_string(),
        }
    }
}

/// Configuration for enabling model thinking/reasoning.
///
/// Each variant carries exactly what its target provider needs.
/// Providers handle the variants they understand and apply sensible
/// defaults for the rest.
#[derive(Clone, Debug)]
pub enum Thinking {
    /// A token budget for thinking. Used by Anthropic.
    BudgetTokens(usize),
    /// A named effort level (e.g. "low", "medium", "high"). Used by OpenAI.
    Effort(String),
    /// Simply enable thinking with no further configuration. Used by Ollama.
    Enabled,
}

impl Thinking {
    pub fn budget_tokens(budget: usize) -> Self {
        Self::BudgetTokens(budget)
    }

    pub fn effort(effort: impl Into<String>) -> Self {
        Self::Effort(effort.into())
    }

    pub fn enabled() -> Self {
        Self::Enabled
    }
}

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

    // Iterates through all remaining chunks and aggregates them.
    // If any error occurs then it will be returned instead.
    pub async fn aggregate(&mut self) -> Result<AggregatedChat, ChatStreamError> {
        let mut result = AggregatedChat::default();

        while let Some(chunk) = self.next().await {
            result.push(&chunk?);
        }

        Ok(result)
    }

    // Iterates through all remaining chunks and aggregates them.
    // Any errors will be ignored.
    pub async fn aggregate_lossy(&mut self) -> AggregatedChat {
        let mut result = AggregatedChat::default();

        while let Some(chunk) = self.next().await {
            if let Ok(chunk) = chunk {
                result.push(&chunk);
            }
        }

        result
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

#[derive(Debug, EnumKind)]
#[enum_kind(ChatChunkKind)]
pub enum ChatChunk {
    Content(String),
    Thinking(String),
}

#[derive(Debug, Default)]
pub struct AggregatedChat {
    pub content: String,
    pub thinking: Option<String>,
}

impl AggregatedChat {
    pub fn push(&mut self, chunk: &ChatChunk) {
        match chunk {
            ChatChunk::Content(text) => self.content.push_str(text),
            ChatChunk::Thinking(text) => {
                self.thinking.get_or_insert_with(String::new).push_str(text);
            }
        }
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
