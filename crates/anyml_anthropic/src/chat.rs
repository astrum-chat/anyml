use anyhow::anyhow;
use anyhttp::HttpClient;
use anyml_core::providers::chat::{
    ChatChunk, ChatError, ChatOptions, ChatProvider, ChatResponse, ChatStreamError,
};
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
        let body = serde_json::to_string(options)
            .map(String::into_bytes)
            .map_err(|this| ChatError::RequestBuildFailed(anyhow::Error::new(this)))?;

        let request = Request::post(format!("{}/v1/messages", self.url))
            .header("anthropic-version", "2023-06-01")
            .header("x-api-key", self.api_key.expose_secret())
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

        Ok(ChatResponse::new(
            stream
                .scan(String::new(), |buffer, chunk| {
                    let chunk = match chunk.map_err(|this| ChatStreamError::ParseError(this)) {
                        Ok(chunk) => chunk,
                        Err(err) => return futures::future::ready(Some(Some(Err(err)))),
                    };

                    let chunk =
                        buffer.drain(..).collect::<String>() + &String::from_utf8_lossy(&chunk);

                    let mut content = String::new();

                    let mut saved_next_event: Option<&str> = None;
                    for (event, next_event) in chunk.split("\n\n").tuple_windows() {
                        saved_next_event = Some(next_event);

                        process_event(event, &mut content);
                    }

                    if let Some(event) = saved_next_event {
                        if event.ends_with("\n\n") {
                            // Full event, process as normal.
                            process_event(event, &mut content);
                        } else {
                            // Partial event, append to buffer.
                            buffer.push_str(&event);
                        }
                    }

                    futures::future::ready(Some(if content.len() == 0 {
                        None
                    } else {
                        Some(Ok(ChatChunk { content }))
                    }))
                })
                .filter_map(async |this| this),
        ))
    }
}

fn process_event(event: &str, content: &mut String) {
    let parsed_chunk =
        parse_event(event).map_err(|this| ChatStreamError::ParseError(anyhow!(this)));

    let parsed_chunk = match parsed_chunk {
        Ok(parsed_chunk) => parsed_chunk,
        Err(_err) => return,
    };

    content.push_str(&parsed_chunk.delta.text);
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
    text: String,
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
                .body("event: content_block_delta\ndata: {\"delta\":{\"text\":\"Hello!\"}}\n\n"),
        );

        let provider = AnthropicProvider::new(client, "test-api-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("claude-3-haiku").messages(messages);

        let mut response = provider.chat(&options).await.unwrap();
        let chunk = response.next().await.unwrap().unwrap();

        assert_eq!(chunk.content, "Hello!");
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
                .body("event: content_block_delta\ndata: {\"delta\":{\"text\":\"Hi\"}}\n\n"),
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
                 event: content_block_delta\ndata: {\"delta\":{\"text\":\"Hello\"}}\n\n\
                 event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ));

        let provider = AnthropicProvider::new(client, "test-api-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("claude-3-haiku").messages(messages);

        let mut response = provider.chat(&options).await.unwrap();
        let chunk = response.next().await.unwrap().unwrap();

        assert_eq!(chunk.content, "Hello");
    }
}
