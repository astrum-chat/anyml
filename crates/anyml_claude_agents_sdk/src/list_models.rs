use anyml_core::{
    models::{Model, ThinkingBudget, ThinkingModes},
    providers::list_models::{ListModelsError, ListModelsProvider},
};

use crate::ClaudeAgentsProvider;

struct StaticModel {
    id: &'static str,
    thinking: Option<StaticThinking>,
}

struct StaticThinking {
    modes: &'static [&'static str],
    budget: Option<ThinkingBudget>,
}

const MODELS: &[StaticModel] = &[
    // Current models
    StaticModel {
        id: "claude-opus-4-6",
        thinking: Some(StaticThinking {
            modes: &["low", "medium", "high", "max"],
            budget: None,
        }),
    },
    StaticModel {
        id: "claude-sonnet-4-6",
        thinking: Some(StaticThinking {
            modes: &["low", "medium", "high"],
            budget: None,
        }),
    },
    StaticModel {
        id: "claude-haiku-4-5",
        thinking: Some(StaticThinking {
            modes: &[],
            budget: Some(ThinkingBudget {
                min: 1024,
                max: 128000,
            }),
        }),
    },
    // Legacy models
    StaticModel {
        id: "claude-sonnet-4-5",
        thinking: Some(StaticThinking {
            modes: &[],
            budget: Some(ThinkingBudget {
                min: 1024,
                max: 128000,
            }),
        }),
    },
    StaticModel {
        id: "claude-opus-4-5",
        thinking: Some(StaticThinking {
            modes: &[],
            budget: Some(ThinkingBudget {
                min: 1024,
                max: 128000,
            }),
        }),
    },
    StaticModel {
        id: "claude-opus-4-1",
        thinking: Some(StaticThinking {
            modes: &[],
            budget: Some(ThinkingBudget {
                min: 1024,
                max: 128000,
            }),
        }),
    },
    StaticModel {
        id: "claude-sonnet-4-0",
        thinking: Some(StaticThinking {
            modes: &[],
            budget: Some(ThinkingBudget {
                min: 1024,
                max: 128000,
            }),
        }),
    },
    StaticModel {
        id: "claude-opus-4-0",
        thinking: Some(StaticThinking {
            modes: &[],
            budget: Some(ThinkingBudget {
                min: 1024,
                max: 128000,
            }),
        }),
    },
];

#[async_trait::async_trait]
impl ListModelsProvider for ClaudeAgentsProvider {
    async fn list_models(&self) -> Result<Vec<Model>, ListModelsError> {
        Ok(MODELS
            .iter()
            .map(|m| Model {
                id: m.id.to_owned(),
                parameters: None,
                quantization: None,
                thinking: m.thinking.as_ref().map(|t| ThinkingModes {
                    modes: t.modes.iter().map(|s| (*s).to_owned()).collect(),
                    budget: t.budget,
                }),
            })
            .collect())
    }
}
