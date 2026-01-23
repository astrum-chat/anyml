use std::env;

use anyml_core::providers::list_models::ListModelsProvider;
use anyml_openai::OpenAiProvider;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::from_filename(".env.test").ok();
    let api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");

    let provider = OpenAiProvider::new(reqwest::Client::new(), api_key);

    let models = provider.list_models().await?;

    println!("Available models:");
    for model in models {
        println!("  - {}", model.id);
    }

    Ok(())
}
