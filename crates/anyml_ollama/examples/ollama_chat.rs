use anyml_core::{
    Message,
    providers::chat::{ChatOptions, ChatProvider, ChatResponse},
};
use anyml_ollama::OllamaProvider;

const MODEL: &str = "qwen2.5:1.5b";
const STREAM_RESPONSE: bool = true;

struct Config {
    chat_provider: Box<dyn ChatProvider>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config {
        chat_provider: Box::new(OllamaProvider::new(reqwest::Client::new())),
    };

    let provider = &config.chat_provider;

    let messages = &[Message::user("Write me a short poem!")];
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
