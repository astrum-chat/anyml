use anyhow::anyhow;
use anyhttp::HttpClient;
use anyml_core::providers::chat::{
    ChatChunk, ChatError, ChatOptions, ChatProvider, ChatResponse, ChatStreamError, Thinking,
};
use anyml_macros::json_string;
use bytes::Bytes;
use futures::StreamExt;
use http::Request;
use itertools::Itertools;
use secrecy::ExposeSecret;
use serde::Deserialize;
use thiserror::Error;

use crate::AnthropicProvider;

#[async_trait::async_trait]
impl<C: HttpClient> ChatProvider for AnthropicProvider<C> {
    async fn chat(&self, options: &ChatOptions<'_>) -> Result<ChatResponse, ChatError> {
        let messages_json = options.messages.to_json();

        let body: String = match &options.thinking {
            Some(Thinking::Effort(effort)) => json_string! {
                "model": options.model,
                "messages": @raw messages_json,
                "stream": options.stream,
                "max_tokens": options.max_tokens,
                "thinking": {
                    "type": "adaptive",
                    "effort": effort
                }
            },
            Some(thinking) => {
                let budget = match thinking {
                    Thinking::BudgetTokens(b) => *b,
                    _ => 10000,
                };
                json_string! {
                    "model": options.model,
                    "messages": @raw messages_json,
                    "stream": options.stream,
                    "max_tokens": options.max_tokens,
                    "thinking": {
                        "type": "enabled",
                        "budget_tokens": budget
                    }
                }
            }
            None => json_string! {
                "model": options.model,
                "messages": @raw messages_json,
                "stream": options.stream,
                "max_tokens": options.max_tokens
            },
        };

        let request = Request::post(format!("{}/v1/messages", self.url))
            .header("anthropic-version", "2023-06-01")
            .header("x-api-key", self.api_key.expose_secret())
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

        Ok(ChatResponse::new(
            stream
                .scan(String::new(), |buffer, chunk| {
                    let chunks = parse_sse_batch(&chunk, buffer);
                    futures::future::ready(Some(chunks))
                })
                .flat_map(futures::stream::iter),
        ))
    }
}

fn parse_sse_batch(
    chunk: &Result<bytes::Bytes, anyhow::Error>,
    buffer: &mut String,
) -> Vec<Result<ChatChunk, ChatStreamError>> {
    let chunk = match chunk {
        Ok(chunk) => chunk,
        Err(err) => return vec![Err(ChatStreamError::ParseError(anyhow!("{err}")))],
    };

    let chunk = buffer.drain(..).collect::<String>() + &String::from_utf8_lossy(chunk);
    let mut results = Vec::new();

    let mut saved_next_event: Option<&str> = None;
    for (event, next_event) in chunk.split("\n\n").tuple_windows() {
        saved_next_event = Some(next_event);
        process_event(event, &mut results);
    }

    if let Some(event) = saved_next_event {
        if event.ends_with("\n\n") {
            process_event(event, &mut results);
        } else {
            buffer.push_str(event);
        }
    }

    results
}

fn process_event(event: &str, results: &mut Vec<Result<ChatChunk, ChatStreamError>>) {
    let parsed = match parse_event(event) {
        Ok(parsed) => parsed,
        Err(_) => return,
    };

    match parsed.delta.r#type.as_str() {
        "thinking_delta" => {
            if let Some(text) = parsed.delta.thinking {
                if !text.is_empty() {
                    results.push(Ok(ChatChunk::Thinking(text)));
                }
            }
        }
        _ => {
            if !parsed.delta.text.is_empty() {
                results.push(Ok(ChatChunk::Content(parsed.delta.text)));
            }
        }
    }
}

fn parse_event(event: &str) -> Result<AnthropicChunkResponse, ParseEventError> {
    let event_body = match event.split_once("event:") {
        Some((_event_prefix, event_body)) => event_body,
        None => {
            return Err(ParseEventError::MissingField { field: "event" });
        }
    };

    let (event_name, event_data) = match event_body.split_once("\n") {
        Some((event_name, event_data)) => (event_name.trim(), event_data),
        None => {
            return Err(ParseEventError::InvalidBody {
                reason: anyhow!("Could not find the name for this event."),
            });
        }
    };

    match event_name {
        "content_block_delta" => parse_content_block_delta_event(event_data),

        _ => Err(ParseEventError::InvalidBody {
            reason: anyhow!("Event has invalid name."),
        }),
    }
}

fn parse_content_block_delta_event(
    event_body: &str,
) -> Result<AnthropicChunkResponse, ParseEventError> {
    let event_data = event_body
        .split("\n")
        .find_map(|field| {
            let field = field.trim();

            if field.starts_with("data:") {
                Some(&field[5..])
            } else {
                None
            }
        })
        .ok_or_else(|| ParseEventError::MissingField { field: "data" })?;

    serde_json::from_str::<AnthropicChunkResponse>(event_data).map_err(|this| {
        ParseEventError::InvalidBody {
            reason: anyhow::Error::new(this),
        }
    })
}

#[derive(Deserialize, Debug)]
struct AnthropicChunkResponse {
    delta: AnthropicChunkResponseDelta,
}

#[derive(Deserialize, Debug)]
struct AnthropicChunkResponseDelta {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    thinking: Option<String>,
}

#[derive(Error, Debug)]
enum ParseEventError {
    #[error("The \"{field}\" field is missing.")]
    MissingField { field: &'static str },

    #[error("This event's body was invalid.")]
    InvalidBody {
        #[source]
        reason: anyhow::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhttp::mock::{MockHttpClient, MockResponse};
    use http::StatusCode;

    #[tokio::test]
    async fn test_chat_success() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body("event: content_block_delta\ndata: {\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello!\"}}\n\n"),
        );

        let provider = AnthropicProvider::new(client, "test-api-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("claude-3-haiku").messages(messages);

        let mut response = provider.chat(&options).await.unwrap();
        let chunk = response.next().await.unwrap().unwrap();

        assert!(matches!(chunk, ChatChunk::Content(ref s) if s == "Hello!"));
    }

    #[tokio::test]
    async fn test_chat_http_error() {
        let client = MockHttpClient::new()
            .with_response(MockResponse::new(StatusCode::UNAUTHORIZED).body("invalid api key"));

        let provider = AnthropicProvider::new(client, "bad-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("claude-3-haiku").messages(messages);

        let result = provider.chat(&options).await;

        assert!(matches!(result, Err(ChatError::RequestError(_))));
    }

    #[tokio::test]
    async fn test_chat_request_headers() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body("event: content_block_delta\ndata: {\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n"),
        );

        let provider = AnthropicProvider::new(client.clone(), "my-secret-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("claude-3-haiku").messages(messages);

        provider.chat(&options).await.unwrap();

        let request = client.last_request().unwrap();
        assert_eq!(request.uri(), "https://api.anthropic.com/v1/messages");
        assert_eq!(request.headers().get("x-api-key").unwrap(), "my-secret-key");
        assert_eq!(
            request.headers().get("anthropic-version").unwrap(),
            "2023-06-01"
        );
    }

    #[tokio::test]
    async fn test_chat_ignores_non_content_events() {
        let client = MockHttpClient::new().with_response(MockResponse::new(StatusCode::OK).body(
            "event: message_start\ndata: {\"type\":\"message_start\"}\n\n\
                 event: content_block_delta\ndata: {\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n\
                 event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ));

        let provider = AnthropicProvider::new(client, "test-api-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("claude-3-haiku").messages(messages);

        let mut response = provider.chat(&options).await.unwrap();
        let chunk = response.next().await.unwrap().unwrap();

        assert!(matches!(chunk, ChatChunk::Content(ref s) if s == "Hello"));
    }

    #[tokio::test]
    async fn test_chat_with_thinking() {
        let client = MockHttpClient::new().with_response(MockResponse::new(StatusCode::OK).body(
            "event: content_block_delta\ndata: {\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Let me reason...\"}}\n\n\
             event: content_block_delta\ndata: {\"delta\":{\"type\":\"text_delta\",\"text\":\"The answer is 42.\"}}\n\n",
        ));

        let provider = AnthropicProvider::new(client, "test-api-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("claude-sonnet-4-20250514")
            .messages(messages)
            .max_tokens(16384)
            .thinking(Thinking::budget_tokens(10000));

        let mut response = provider.chat(&options).await.unwrap();
        let result = response.aggregate().await.unwrap();

        assert_eq!(result.thinking.as_deref(), Some("Let me reason..."));
        assert_eq!(result.content, "The answer is 42.");
    }
}
