use anyml_core::providers::list_models::ListModelsProvider;
use anyml_ollama::OllamaProvider;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let provider = OllamaProvider::new(reqwest::Client::new());

    let models = provider.list_models().await?;

    println!("Available models:");
    for model in models {
        println!("  - {}", model.id);
    }

    Ok(())
}
