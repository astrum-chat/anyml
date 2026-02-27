use anyhow::anyhow;
use anyhttp::HttpClient;
use anyml_core::{
    models::{Model, ThinkingBudget, ThinkingModes},
    providers::list_models::{ListModelsError, ListModelsProvider},
};
use bytes::Bytes;
use http::Request;
use phf::phf_map;
use secrecy::ExposeSecret;
use serde::Deserialize;

use crate::AnthropicProvider;

type StaticThinkingModes = ThinkingModes<&'static [&'static str]>;

static THINKING_MODELS: phf::Map<&'static str, StaticThinkingModes> = phf_map! {
    "claude-3-7-sonnet-20250219" => StaticThinkingModes { modes: &[], budget: Some(ThinkingBudget { min: 1024, max: 128000 }) },
    "claude-sonnet-4-20250514" => StaticThinkingModes { modes: &[], budget: Some(ThinkingBudget { min: 1024, max: 128000 }) },
    "claude-sonnet-4-5-20250929" => StaticThinkingModes { modes: &[], budget: Some(ThinkingBudget { min: 1024, max: 128000 }) },
    "claude-sonnet-4-6" => StaticThinkingModes { modes: &["low", "medium", "high", "max"], budget: Some(ThinkingBudget { min: 1024, max: 128000 }) },
    "claude-opus-4-20250514" => StaticThinkingModes { modes: &[], budget: Some(ThinkingBudget { min: 1024, max: 128000 }) },
    "claude-opus-4-1-20250805" => StaticThinkingModes { modes: &[], budget: Some(ThinkingBudget { min: 1024, max: 128000 }) },
    "claude-opus-4-5-20251101" => StaticThinkingModes { modes: &[], budget: Some(ThinkingBudget { min: 1024, max: 128000 }) },
    "claude-opus-4-6" => StaticThinkingModes { modes: &["low", "medium", "high", "max"], budget: None },
    "claude-haiku-4-5-20251001" => StaticThinkingModes { modes: &[], budget: Some(ThinkingBudget { min: 1024, max: 128000 }) },
};

#[async_trait::async_trait]
impl<C: HttpClient> ListModelsProvider for AnthropicProvider<C> {
    async fn list_models(&self) -> Result<Vec<Model>, ListModelsError> {
        let request = Request::get(format!("{}/v1/models", self.url))
            .header("anthropic-version", "2023-06-01")
            .header("x-api-key", self.api_key.expose_secret())
            .body(Vec::new())
            .map_err(|e| ListModelsError::RequestBuildFailed(anyhow::Error::new(e)))?;

        let response = self
            .client
            .execute(request)
            .await
            .map_err(|e| ListModelsError::ResponseFetchFailed(e))?;

        if !response.status().is_success() {
            let err_body = response
                .bytes()
                .await
                .unwrap_or_else(|_| Bytes::from_static(b"<failed to read>"));

            return Err(ListModelsError::ResponseFetchFailed(anyhow!(
                String::from_utf8_lossy(&err_body).into_owned()
            )));
        }

        let body = response
            .bytes()
            .await
            .map_err(|e| ListModelsError::ResponseFetchFailed(e))?;

        let anthropic_response: AnthropicModelsResponse = serde_json::from_slice(&body)
            .map_err(|e| ListModelsError::ParseError(anyhow::Error::new(e)))?;

        let models = anthropic_response
            .data
            .into_iter()
            .map(|m| {
                let thinking =
                    THINKING_MODELS
                        .get(m.id.as_str())
                        .map(|s| ThinkingModes {
                            modes: s.modes.iter().map(|s| (*s).into()).collect(),
                            budget: s.budget,
                        });
                Model {
                    id: m.id,
                    parameters: None,
                    quantization: None,
                    thinking,
                }
            })
            .collect();

        Ok(models)
    }
}

#[derive(Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModel>,
}

#[derive(Deserialize)]
struct AnthropicModel {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhttp::mock::{MockHttpClient, MockResponse};
    use http::StatusCode;

    #[tokio::test]
    async fn test_list_models_success() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body(r#"{"data":[{"id":"claude-3-opus"},{"id":"claude-3-sonnet"}]}"#),
        );

        let provider = AnthropicProvider::new(client, "test-api-key");
        let models = provider.list_models().await.unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "claude-3-opus");
        assert_eq!(models[1].id, "claude-3-sonnet");
    }

    #[tokio::test]
    async fn test_list_models_empty() {
        let client = MockHttpClient::new()
            .with_response(MockResponse::new(StatusCode::OK).body(r#"{"data":[]}"#));

        let provider = AnthropicProvider::new(client, "test-api-key");
        let models = provider.list_models().await.unwrap();

        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn test_list_models_unauthorized() {
        let client = MockHttpClient::new()
            .with_response(MockResponse::new(StatusCode::UNAUTHORIZED).body("invalid api key"));

        let provider = AnthropicProvider::new(client, "bad-key");
        let result = provider.list_models().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_models_invalid_json() {
        let client = MockHttpClient::new()
            .with_response(MockResponse::new(StatusCode::OK).body("not valid json"));

        let provider = AnthropicProvider::new(client, "test-api-key");
        let result = provider.list_models().await;

        assert!(matches!(result, Err(ListModelsError::ParseError(_))));
    }

    #[tokio::test]
    async fn test_list_models_request_headers() {
        let client = MockHttpClient::new()
            .with_response(MockResponse::new(StatusCode::OK).body(r#"{"data":[]}"#));

        let provider = AnthropicProvider::new(client.clone(), "my-secret-key");
        provider.list_models().await.unwrap();

        let request = client.last_request().unwrap();
        assert_eq!(request.uri(), "https://api.anthropic.com/v1/models");
        assert_eq!(request.headers().get("x-api-key").unwrap(), "my-secret-key");
        assert_eq!(
            request.headers().get("anthropic-version").unwrap(),
            "2023-06-01"
        );
    }
}
