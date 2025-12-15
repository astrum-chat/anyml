use anyhow::anyhow;
use anyhttp::HttpClient;
use anyml_core::{
    ChatError, ChatOptions, ChatProvider, ChatResponse, ChatStreamError, ChunkResponse,
};
use bytes::Bytes;
use futures::StreamExt;
use http::Request;
use itertools::Itertools;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::borrow::Cow;
use thiserror::Error;

const DEFAULT_URL: &'static str = "https://api.anthropic.com";

pub struct AnthropicProvider<C: HttpClient> {
    client: C,
    url: Cow<'static, str>,
    api_key: SecretString,
}

impl<C: HttpClient> AnthropicProvider<C> {
    pub fn new(client: C, api_key: impl Into<SecretString>) -> Self {
        Self {
            client,
            url: Cow::Borrowed(DEFAULT_URL),
            api_key: api_key.into(),
        }
    }

    pub fn url(mut self, url: impl Into<Cow<'static, str>>) -> Self {
        self.url = url.into();
        self
    }

    pub fn api_key(mut self, api_key: impl Into<SecretString>) -> Self {
        self.api_key = api_key.into();
        self
    }
}

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

        Ok(Box::pin(
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
                        Some(Ok(ChunkResponse { content }))
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
        Err(_err) => return, /* futures::future::ready(Some(Err(err)))*/
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
