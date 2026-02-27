use anyhow::anyhow;
use anyhttp::HttpClient;
use anyml_core::{
    models::Message,
    providers::chat::{
        ChatChunk, ChatError, ChatOptions, ChatProvider, ChatResponse, ChatStreamError,
    },
};
use anyml_macros::json_string;
use bytes::Bytes;
use futures::StreamExt;
use http::Request;
use serde::Deserialize;

use crate::OllamaProvider;

#[async_trait::async_trait]
impl<C: HttpClient> ChatProvider for OllamaProvider<C> {
    async fn chat(&self, options: &ChatOptions<'_>) -> Result<ChatResponse, ChatError> {
        let messages_json = options.messages.to_json();

        let body: String = json_string! {
            "model": options.model,
            "messages": @raw messages_json,
            "stream": options.stream,
            if options.thinking.is_some() {
                "think": true
            }
        };

        let request = Request::post(format!("{}/api/chat", self.url))
            .body(body.into_bytes())
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

        // When thinking is enabled, Ollama embeds thinking in <think>...</think> tags
        // within the content field. We use a state machine to separate them.
        Ok(ChatResponse::new(
            stream
                .scan(false, |in_thinking, chunk| {
                    let chunks = parse_chunk(&chunk, in_thinking);
                    futures::future::ready(Some(chunks))
                })
                .flat_map(futures::stream::iter),
        ))
    }
}

fn parse_chunk(
    chunk: &Result<bytes::Bytes, anyhow::Error>,
    in_thinking: &mut bool,
) -> Vec<Result<ChatChunk, ChatStreamError>> {
    let chunk = match chunk {
        Ok(chunk) => chunk,
        Err(err) => return vec![Err(ChatStreamError::ParseError(anyhow!("{err}")))],
    };

    let response: OllamaChunkResponse = match serde_json::from_slice(chunk) {
        Ok(r) => r,
        Err(e) => return vec![Err(ChatStreamError::ParseError(anyhow::Error::new(e)))],
    };

    let (content, thinking) = split_thinking(&response.message.content, in_thinking);

    let mut results = Vec::new();
    if let Some(thinking) = thinking {
        if !thinking.is_empty() {
            results.push(Ok(ChatChunk::Thinking(thinking)));
        }
    }
    if !content.is_empty() {
        results.push(Ok(ChatChunk::Content(content)));
    }
    results
}

/// Separates `<think>...</think>` tagged content from regular content.
/// Tracks state across calls via `in_thinking`.
fn split_thinking(raw: &str, in_thinking: &mut bool) -> (String, Option<String>) {
    let mut content = String::new();
    let mut thinking: Option<String> = None;
    let mut remaining = raw;

    while !remaining.is_empty() {
        if *in_thinking {
            if let Some(end) = remaining.find("</think>") {
                let think_text = &remaining[..end];
                if !think_text.is_empty() {
                    thinking.get_or_insert_with(String::new).push_str(think_text);
                }
                *in_thinking = false;
                remaining = &remaining[end + 8..];
            } else {
                thinking.get_or_insert_with(String::new).push_str(remaining);
                break;
            }
        } else if let Some(start) = remaining.find("<think>") {
            content.push_str(&remaining[..start]);
            *in_thinking = true;
            remaining = &remaining[start + 7..];
        } else {
            content.push_str(remaining);
            break;
        }
    }

    (content, thinking)
}

#[derive(Deserialize)]
struct OllamaChunkResponse {
    message: Message,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhttp::mock::{MockHttpClient, MockResponse};
    use anyml_core::providers::chat::Thinking;
    use http::StatusCode;

    #[tokio::test]
    async fn test_chat_success() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body(r#"{"message":{"role":"assistant","content":"Hello!"}}"#),
        );

        let provider = OllamaProvider::new(client);
        let messages = &["Hi".into()];
        let options = ChatOptions::new("llama2").messages(messages);

        let mut response = provider.chat(&options).await.unwrap();
        let chunk = response.next().await.unwrap().unwrap();

        assert!(matches!(chunk, ChatChunk::Content(ref s) if s == "Hello!"));
    }

    #[tokio::test]
    async fn test_chat_http_error() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::INTERNAL_SERVER_ERROR).body("server error"),
        );

        let provider = OllamaProvider::new(client);
        let messages = &["Hi".into()];
        let options = ChatOptions::new("llama2").messages(messages);

        let result = provider.chat(&options).await;

        assert!(matches!(result, Err(ChatError::RequestError(_))));
    }

    #[tokio::test]
    async fn test_chat_request_url() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body(r#"{"message":{"role":"assistant","content":"Hi"}}"#),
        );

        let provider = OllamaProvider::new(client.clone());
        let messages = &["Hi".into()];
        let options = ChatOptions::new("llama2").messages(messages);

        provider.chat(&options).await.unwrap();

        let request = client.last_request().unwrap();
        assert_eq!(request.uri(), "http://localhost:11434/api/chat");
    }

    #[tokio::test]
    async fn test_chat_aggregate() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body(r#"{"message":{"role":"assistant","content":"Hello"}}"#),
        );

        let provider = OllamaProvider::new(client);
        let messages = &["Hi".into()];
        let options = ChatOptions::new("llama2").messages(messages);

        let mut response = provider.chat(&options).await.unwrap();
        let aggregated = response.aggregate().await.unwrap();

        assert_eq!(aggregated.content, "Hello");
    }

    #[tokio::test]
    async fn test_chat_with_thinking_complete_block() {
        // A single chunk containing a complete <think>...</think> block and text after.
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body(r#"{"message":{"role":"assistant","content":"<think>I need to think</think>The answer."}}"#),
        );

        let provider = OllamaProvider::new(client);
        let messages = &["Hi".into()];
        let options = ChatOptions::new("deepseek-r1:7b")
            .messages(messages)
            .thinking(Thinking::enabled());

        let mut response = provider.chat(&options).await.unwrap();
        let result = response.aggregate().await.unwrap();

        assert_eq!(result.thinking.as_deref(), Some("I need to think"));
        assert_eq!(result.content, "The answer.");
    }

    #[tokio::test]
    async fn test_chat_without_thinking_no_tags() {
        // Without thinking enabled, content passes through normally.
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body(r#"{"message":{"role":"assistant","content":"The answer."}}"#),
        );

        let provider = OllamaProvider::new(client);
        let messages = &["Hi".into()];
        let options = ChatOptions::new("llama2").messages(messages);

        let mut response = provider.chat(&options).await.unwrap();
        let chunk = response.next().await.unwrap().unwrap();

        assert!(matches!(chunk, ChatChunk::Content(ref s) if s == "The answer."));
    }
}
