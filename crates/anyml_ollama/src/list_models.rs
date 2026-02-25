use anyhow::anyhow;
use anyhttp::HttpClient;
use anyml_core::{
    models::{Model, ModelParams, ModelQuant},
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

        let models = ollama_response
            .models
            .into_iter()
            .map(|m| {
                let (parameters, quantization) = m
                    .details
                    .map(|d| {
                        let params = d.parameter_size.map(|p| ModelParams::new(&p));
                        let quant = d.quantization_level.map(|q| ModelQuant::new(&q));
                        (params, quant)
                    })
                    .unwrap_or((None, None));
                Model {
                    id: m.name,
                    parameters,
                    quantization,
                }
            })
            .collect();

        Ok(models)
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhttp::mock::{MockHttpClient, MockResponse};
    use http::StatusCode;

    #[tokio::test]
    async fn test_list_models_success() {
        let client = MockHttpClient::new().with_response(
            MockResponse::new(StatusCode::OK)
                .body(r#"{"models":[{"name":"llama2"},{"name":"codellama"}]}"#),
        );

        let provider = OllamaProvider::new(client);
        let models = provider.list_models().await.unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "llama2");
        assert_eq!(models[1].id, "codellama");
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
}
