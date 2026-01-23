use anyhow::anyhow;
use anyhttp::HttpClient;
use anyml_core::{
    models::Message,
    providers::chat::{
        ChatChunk, ChatError, ChatOptions, ChatProvider, ChatResponse, ChatStreamError,
    },
};
use bytes::Bytes;
use futures::StreamExt;
use http::Request;
use serde::Deserialize;

use crate::OllamaProvider;

#[async_trait::async_trait]
impl<C: HttpClient> ChatProvider for OllamaProvider<C> {
    async fn chat(&self, options: &ChatOptions<'_>) -> Result<ChatResponse, ChatError> {
        let body = serde_json::to_string(options)
            .map(String::into_bytes)
            .map_err(|this| ChatError::RequestBuildFailed(anyhow::Error::new(this)))?;

        let request = Request::post(format!("{}/api/chat", self.url))
            .body(body)
            .map_err(|this| ChatError::RequestBuildFailed(anyhow::Error::new(this)))?;

        let response = self
            .client
            .execute(request)
            .await
            .map_err(|this| ChatError::ResponseFetchFailed(this))?;

        if !response.status().is_success() {
            let err_body = response
                .bytes()
                .await
                .unwrap_or_else(|_| Bytes::from_static(b"<failed to read>"));

            return Err(ChatError::RequestError(anyhow!(
                String::from_utf8_lossy(&err_body).into_owned()
            )));
        }

        let stream = response.bytes_stream();

        Ok(ChatResponse::new(stream.map(|chunk| {
            let chunk = chunk.map_err(ChatStreamError::ParseError)?;

            let response: OllamaChunkResponse = serde_json::from_slice(&chunk)
                .map_err(|e| ChatStreamError::ParseError(anyhow::Error::new(e)))?;

            Ok(ChatChunk {
                content: response.message.content,
            })
        })))
    }
}

#[derive(Deserialize)]
struct OllamaChunkResponse {
    message: Message,
}
