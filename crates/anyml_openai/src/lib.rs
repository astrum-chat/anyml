use std::borrow::Cow;

use anyhow::anyhow;
use anyhttp::HttpClient;
use anyml_core::{
    ChatError, ChatOptions, ChatProvider, ChatResponse, ChatStreamError, ChunkResponse,
};
use bytes::Bytes;
use futures::StreamExt;
use http::Request;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use smallvec::SmallVec;

const DEFAULT_URL: &'static str = "https://api.openai.com";
const OPEN_ROUTER_URL: &'static str = "https://openrouter.ai/api";

pub struct OpenAiProvider<C: HttpClient> {
    client: C,
    url: Cow<'static, str>,
    api_key: SecretString,
}

impl<C: HttpClient> OpenAiProvider<C> {
    pub fn new(client: C, api_key: impl Into<SecretString>) -> Self {
        Self {
            client,
            url: Cow::Borrowed(DEFAULT_URL),
            api_key: api_key.into(),
        }
    }

    pub fn open_router(client: C, api_key: impl Into<SecretString>) -> Self {
        Self {
            client,
            url: Cow::Borrowed(OPEN_ROUTER_URL),
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
impl<C: HttpClient> ChatProvider for OpenAiProvider<C> {
    async fn chat(&self, options: &ChatOptions<'_>) -> Result<ChatResponse, ChatError> {
        let options = options.clone().extra("reasoning", {
            let mut map = serde_json::Map::new();
            map.insert("effort".into(), "none".into());
            map
        });

        let body = serde_json::to_string(&options)
            .map(String::into_bytes)
            .map_err(|this| ChatError::RequestBuildFailed(anyhow::Error::new(this)))?;

        let request = Request::post(format!("{}/v1/chat/completions", self.url))
            .header(
                "Authorization",
                format!("Bearer {}", self.api_key.expose_secret()),
            )
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

        Ok(Box::pin(stream.filter_map(|chunk| async {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(err) => return Some(Err(ChatStreamError::ParseError(err))),
            };
            let chunk = String::from_utf8_lossy(&chunk);

            let mut parsed_chunk = ChunkResponse {
                content: String::new(),
            };

            for event in chunk.split("\n\n") {
                if let Some(event_body) = event.strip_prefix("data:") {
                    let parsed_event =
                        match serde_json::from_str::<OpenAiChunkResponse>(&event_body) {
                            Ok(parsed_event) => parsed_event,
                            Err(err) => {
                                return Some(Err(ChatStreamError::ParseError(anyhow::Error::new(
                                    err,
                                ))));
                            }
                        };

                    if let Some(content) = parsed_event
                        .choices
                        .iter()
                        .next()
                        .map(|this| &this.delta.content)
                    {
                        parsed_chunk.content += &content;
                    }
                }
            }

            Some(Ok(parsed_chunk))
        })))
    }
}

#[derive(Deserialize)]
struct OpenAiChunkResponse {
    choices: SmallVec<[OpenAiChunkResponseChoice; 1]>,
}

#[derive(Deserialize)]
struct OpenAiChunkResponseChoice {
    delta: OpenAiChunkResponseDelta,
}

#[derive(Deserialize)]
struct OpenAiChunkResponseDelta {
    content: String,
}
