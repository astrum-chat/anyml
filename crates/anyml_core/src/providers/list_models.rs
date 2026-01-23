use thiserror::Error;

use crate::models::Model;

#[async_trait::async_trait]
pub trait ListModelsProvider: Send + Sync {
    async fn list_models(&self) -> Result<Vec<Model>, ListModelsError>;
}

#[derive(Debug, Error)]
pub enum ListModelsError {
    #[error("Failed to build the request: {0}.")]
    RequestBuildFailed(#[source] anyhow::Error),

    #[error("Failed to retrieve the response: {0}.")]
    ResponseFetchFailed(#[source] anyhow::Error),

    #[error("Failed to parse response: {0}.")]
    ParseError(#[source] anyhow::Error),
}
