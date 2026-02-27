use anyhow::anyhow;
use anyhttp::HttpClient;
use anyml_core::{
    models::{Model, ModelParams, ModelQuant, ThinkingModes},
    providers::list_models::{ListModelsError, ListModelsProvider},
};
use bytes::Bytes;
use http::Request;
use serde::Deserialize;

use crate::OllamaProvider;

#[async_trait::async_trait]
impl<C: HttpClient> ListModelsProvider for OllamaProvider<C> {
    async fn list_models(&self) -> Result<Vec<Model>, ListModelsError> {
        let request = Request::get(format!("{}/api/tags", self.url))
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

        let ollama_response: OllamaTagsResponse = serde_json::from_slice(&body)
            .map_err(|e| ListModelsError::ParseError(anyhow::Error::new(e)))?;

        let mut models = Vec::with_capacity(ollama_response.models.len());
        for m in ollama_response.models {
            let (parameters, quantization) = m
                .details
                .map(|d| {
                    let params = d.parameter_size.map(|p| ModelParams::new(&p));
                    let quant = d.quantization_level.map(|q| ModelQuant::new(&q));
                    (params, quant)
                })
                .unwrap_or((None, None));

            let thinking = self.fetch_thinking_modes(&m.name).await;

            models.push(Model {
                id: m.name,
                parameters,
                quantization,
                thinking,
            });
        }

        Ok(models)
    }
}

impl<C: HttpClient> OllamaProvider<C> {
    /// Calls `/api/show` for a model and returns `ThinkingModes` if the model
    /// has the `"thinking"` capability. Returns `None` on any error or if
    /// the capability is absent.
    async fn fetch_thinking_modes(&self, model: &str) -> Option<ThinkingModes> {
        let body = format!(r#"{{"model":"{}"}}"#, model);
        let request = Request::post(format!("{}/api/show", self.url))
            .body(body.into_bytes())
            .ok()?;

        let response = self.client.execute(request).await.ok()?;

        if !response.status().is_success() {
            return None;
        }

        let bytes = response.bytes().await.ok()?;
        let show: OllamaShowResponse = serde_json::from_slice(&bytes).ok()?;

        if show.capabilities.contains(&"thinking".to_string()) {
            Some(ThinkingModes {
                modes: vec!["enabled".into()],
                budget: None,
            })
        } else {
            None
        }
    }
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
    details: Option<OllamaModelDetails>,
}

#[derive(Deserialize)]
struct OllamaModelDetails {
    parameter_size: Option<String>,
    quantization_level: Option<String>,
}

#[derive(Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    capabilities: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhttp::mock::{MockHttpClient, MockResponse};
    use http::StatusCode;

    #[tokio::test]
    async fn test_list_models_success() {
        let client = MockHttpClient::new()
            .with_response(
                MockResponse::new(StatusCode::OK)
                    .body(r#"{"models":[{"name":"llama2"},{"name":"codellama"}]}"#),
            )
            // /api/show for llama2
            .with_response(MockResponse::new(StatusCode::OK).body(r#"{"capabilities":[]}"#))
            // /api/show for codellama
            .with_response(MockResponse::new(StatusCode::OK).body(r#"{"capabilities":[]}"#));

        let provider = OllamaProvider::new(client);
        let models = provider.list_models().await.unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "llama2");
        assert!(models[0].thinking.is_none());
        assert_eq!(models[1].id, "codellama");
        assert!(models[1].thinking.is_none());
    }

    #[tokio::test]
    async fn test_list_models_with_thinking_model() {
        let client = MockHttpClient::new()
            .with_response(
                MockResponse::new(StatusCode::OK)
                    .body(r#"{"models":[{"name":"deepseek-r1:7b"},{"name":"llama2"}]}"#),
            )
            // /api/show for deepseek-r1:7b — has thinking capability
            .with_response(
                MockResponse::new(StatusCode::OK)
                    .body(r#"{"capabilities":["completion","thinking"]}"#),
            )
            // /api/show for llama2 — no thinking
            .with_response(MockResponse::new(StatusCode::OK).body(r#"{"capabilities":["completion"]}"#));

        let provider = OllamaProvider::new(client);
        let models = provider.list_models().await.unwrap();

        assert_eq!(models.len(), 2);
        assert!(models[0].thinking.is_some());
        assert_eq!(models[0].thinking.as_ref().unwrap().modes, vec!["enabled"]);
        assert!(models[1].thinking.is_none());
    }

    #[tokio::test]
    async fn test_list_models_empty() {
        let client = MockHttpClient::new()
            .with_response(MockResponse::new(StatusCode::OK).body(r#"{"models":[]}"#));

        let provider = OllamaProvider::new(client);
        let models = provider.list_models().await.unwrap();

        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn test_list_models_http_error() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::INTERNAL_SERVER_ERROR).body("server error"),
        );

        let provider = OllamaProvider::new(client);
        let result = provider.list_models().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_models_invalid_json() {
        let client = MockHttpClient::new()
            .with_response(MockResponse::new(StatusCode::OK).body("not valid json"));

        let provider = OllamaProvider::new(client);
        let result = provider.list_models().await;

        assert!(matches!(result, Err(ListModelsError::ParseError(_))));
    }

    #[tokio::test]
    async fn test_list_models_request_url() {
        let client = MockHttpClient::new()
            .with_response(MockResponse::new(StatusCode::OK).body(r#"{"models":[]}"#));

        let provider = OllamaProvider::new(client.clone());
        provider.list_models().await.unwrap();

        let request = client.last_request().unwrap();
        assert_eq!(request.uri(), "http://localhost:11434/api/tags");
    }

    #[tokio::test]
    async fn test_list_models_show_failure_graceful() {
        // If /api/show fails, thinking should be None (not an error)
        let client = MockHttpClient::new()
            .with_response(
                MockResponse::new(StatusCode::OK)
                    .body(r#"{"models":[{"name":"llama2"}]}"#),
            )
            // /api/show returns error
            .with_response(MockResponse::new(StatusCode::INTERNAL_SERVER_ERROR).body("error"));

        let provider = OllamaProvider::new(client);
        let models = provider.list_models().await.unwrap();

        assert_eq!(models.len(), 1);
        assert!(models[0].thinking.is_none());
    }
}
