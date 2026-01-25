pub mod chat;
pub mod list_models;

pub use chat::{ChatChunk, ChatError, ChatOptions, ChatProvider, ChatResponse, ChatStreamError};
pub use list_models::{ListModelsError, ListModelsProvider};

impl<T: ChatProvider + ListModelsProvider> ProviderTrait for T {}
