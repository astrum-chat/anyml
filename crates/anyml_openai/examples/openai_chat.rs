use std::env;

use anyml_core::providers::chat::{ChatOptions, ChatProvider, ChatResponse};
use anyml_openai::OpenAiProvider;

const MODEL: &str = "deepseek/deepseek-chat-v3-0324";
const STREAM_RESPONSE: bool = false;

struct Config {
    chat_provider: Box<dyn ChatProvider>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::from_filename(".env.test").ok();
    let api_key = env::var("OPENROUTER_API_KEY").expect("API_KEY not set");

    let config = Config {
        chat_provider: Box::new(OpenAiProvider::open_router(reqwest::Client::new(), api_key)),
    };

    let provider = &config.chat_provider;

    let messages = &["Write me a short poem".into()];
    let options = ChatOptions::new(&MODEL).messages(messages);

    let response = provider.chat(&options).await.map_err(anyhow::Error::new)?;

    if STREAM_RESPONSE {
        stream_response(response).await;
    } else {
        println!("{}", collect_response(response).await)
    }

    Ok(())
}

/// Streams the response to tokio::io::stdout.
async fn stream_response(mut response: ChatResponse<'_>) {
    use tokio::io::{AsyncWriteExt, stdout};

    let mut out = stdout();
    while let Some(Ok(chunk)) = response.next().await {
        out.write_all(chunk.content.as_bytes()).await.unwrap();
        out.flush().await.unwrap();
    }
}

/// Collects all chunks in the response stream to a string.
async fn collect_response(mut response: ChatResponse<'_>) -> String {
    response.aggregate_lossy().await.unwrap_or_default().content
}
