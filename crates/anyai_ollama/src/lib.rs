use anyai::{
    ChatError, ChatOptions, ChatProvider, ChatResponse, ChatStreamError, Message, ParseMessageError,
};
use anyhttp::HttpClient;
use futures::StreamExt;
use http::Request;
use serde_json::Value;
pub struct OllamaProvider<C: HttpClient> {
    url: String,
    client: C,
}

impl<C: HttpClient> OllamaProvider<C> {
    pub fn new(url: impl Into<String>, client: C) -> Self {
        Self {
            url: url.into(),
            client,
        }
    }
}

#[async_trait::async_trait]
impl<C: HttpClient> ChatProvider for OllamaProvider<C> {
    async fn chat(&self, options: &ChatOptions<'_>) -> Result<ChatResponse, ChatError> {
        let body = options
            .to_json("user", "assistant", "system", "tool")
            .map(String::into_bytes)
            .map_err(|this| ChatError::RequestBuildFailed(anyhow::Error::new(this)))?;

        let request = Request::post(self.url.clone() + "/api/chat")
            .body(body)
            .map_err(|this| ChatError::RequestBuildFailed(anyhow::Error::new(this)))?;

        let response = self
            .client
            .execute(request)
            .await
            .map_err(|this| ChatError::ResponseFetchFailed(this))?;

        Ok(Box::pin(response.bytes_stream().map(|chunk| {
            let chunk = chunk.map_err(ChatStreamError::ParseError)?;

            let json: Value = serde_json::from_slice(&chunk)
                .map_err(|e| ChatStreamError::ParseError(anyhow::Error::new(e)))?;

            let response = OllamaChunkResponse::from_json(&json)
                .map_err(|e| ChatStreamError::ParseError(anyhow::Error::new(e)))?;

            Ok(response.message.content)
        })))
    }
}

struct OllamaChunkResponse {
    message: Message,
}

impl OllamaChunkResponse {
    pub fn from_json(value: &serde_json::Value) -> Result<Self, ParseMessageError> {
        let message_value = value
            .get("message")
            .ok_or_else(|| ParseMessageError::MissingField("message"))?;

        let message = Message::from_json(message_value, "user", "assistant", "system")?;

        Ok(Self { message })
    }
}
