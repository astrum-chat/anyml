use anyhow::anyhow;
use anyhttp::HttpClient;
use anyml_core::providers::chat::{
    ChatChunk, ChatError, ChatOptions, ChatProvider, ChatResponse, ChatStreamError, Thinking,
};
use anyml_macros::json_string;
use bytes::Bytes;
use futures::StreamExt;
use http::Request;
use secrecy::ExposeSecret;
use serde::Deserialize;
use smallvec::SmallVec;

use crate::OpenAiProvider;

#[async_trait::async_trait]
impl<C: HttpClient> ChatProvider for OpenAiProvider<C> {
    async fn chat(&self, options: &ChatOptions<'_>) -> Result<ChatResponse, ChatError> {
        let messages_json = options.messages.to_json();

        let body: String = match &options.thinking {
            Some(Thinking::Effort(effort)) => json_string! {
                "model": options.model,
                "messages": @raw messages_json,
                "stream": options.stream,
                "max_completion_tokens": options.max_tokens,
                "reasoning_effort": effort
            },
            Some(_) => json_string! {
                "model": options.model,
                "messages": @raw messages_json,
                "stream": options.stream,
                "max_completion_tokens": options.max_tokens,
                "reasoning_effort": "medium"
            },
            None => json_string! {
                "model": options.model,
                "messages": @raw messages_json,
                "stream": options.stream,
                "max_tokens": options.max_tokens
            },
        };

        let request = Request::post(format!("{}/v1/chat/completions", self.url))
            .header(
                "Authorization",
                format!("Bearer {}", self.api_key.expose_secret()),
            )
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
                .map(parse_sse_chunk)
                .flat_map(futures::stream::iter),
        ))
    }
}

fn parse_sse_chunk(
    chunk: Result<bytes::Bytes, anyhow::Error>,
) -> Vec<Result<ChatChunk, ChatStreamError>> {
    let chunk = match chunk {
        Ok(chunk) => chunk,
        Err(err) => return vec![Err(ChatStreamError::ParseError(err))],
    };
    let chunk = String::from_utf8_lossy(&chunk);

    let mut results = Vec::new();

    for event in chunk.split("\n\n") {
        if let Some(event_body) = event.strip_prefix("data:") {
            let parsed_event = match serde_json::from_str::<OpenAiChunkResponse>(event_body) {
                Ok(parsed_event) => parsed_event,
                Err(err) => {
                    results.push(Err(ChatStreamError::ParseError(anyhow::Error::new(err))));
                    continue;
                }
            };

            if let Some(choice) = parsed_event.choices.first() {
                if let Some(ref reasoning) = choice.delta.reasoning_content {
                    if !reasoning.is_empty() {
                        results.push(Ok(ChatChunk::Thinking(reasoning.clone())));
                    }
                }
                if !choice.delta.content.is_empty() {
                    results.push(Ok(ChatChunk::Content(choice.delta.content.clone())));
                }
            }
        }
    }

    results
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
    #[serde(default)]
    content: String,
    #[serde(default)]
    reasoning_content: Option<String>,
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
                .body("data:{\"choices\":[{\"delta\":{\"content\":\"Hello!\"}}]}\n\n"),
        );

        let provider = OpenAiProvider::new(client, "test-api-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("gpt-4").messages(messages);

        let mut response = provider.chat(&options).await.unwrap();
        let chunk = response.next().await.unwrap().unwrap();

        assert!(matches!(chunk, ChatChunk::Content(ref s) if s == "Hello!"));
    }

    #[tokio::test]
    async fn test_chat_http_error() {
        let client = MockHttpClient::new()
            .with_response(MockResponse::new(StatusCode::UNAUTHORIZED).body("invalid api key"));

        let provider = OpenAiProvider::new(client, "bad-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("gpt-4").messages(messages);

        let result = provider.chat(&options).await;

        assert!(matches!(result, Err(ChatError::RequestError(_))));
    }

    #[tokio::test]
    async fn test_chat_request_headers() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body("data:{\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n"),
        );

        let provider = OpenAiProvider::new(client.clone(), "my-secret-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("gpt-4").messages(messages);

        provider.chat(&options).await.unwrap();

        let request = client.last_request().unwrap();
        assert_eq!(request.uri(), "https://api.openai.com/v1/chat/completions");
        assert_eq!(
            request.headers().get("Authorization").unwrap(),
            "Bearer my-secret-key"
        );
    }

    #[tokio::test]
    async fn test_chat_open_router() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body("data:{\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n"),
        );

        let provider = OpenAiProvider::open_router(client.clone(), "router-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("gpt-4").messages(messages);

        provider.chat(&options).await.unwrap();

        let request = client.last_request().unwrap();
        assert_eq!(
            request.uri(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[tokio::test]
    async fn test_chat_with_reasoning_content() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body("data:{\"choices\":[{\"delta\":{\"content\":\"\",\"reasoning_content\":\"Let me think...\"}}]}\n\ndata:{\"choices\":[{\"delta\":{\"content\":\"Hello!\",\"reasoning_content\":null}}]}\n\n"),
        );

        let provider = OpenAiProvider::new(client, "test-api-key");
        let messages = &["Hi".into()];
        let options = ChatOptions::new("o3-mini")
            .messages(messages)
            .thinking(Thinking::effort("high"));

        let mut response = provider.chat(&options).await.unwrap();
        let result = response.aggregate().await.unwrap();

        assert_eq!(result.content, "Hello!");
        assert_eq!(result.thinking.as_deref(), Some("Let me think..."));
    }
}
