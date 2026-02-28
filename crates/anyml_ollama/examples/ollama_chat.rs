use anyml_core::{
    Message,
    providers::chat::{ChatChunk, ChatOptions, ChatProvider, ChatResponse},
};
use anyml_ollama::OllamaProvider;

const MODEL: &str = "deepseek-r1:8b";
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
    let options = ChatOptions::new(&MODEL)
        .thinking(anyml_core::Thinking::Enabled)
        .messages(messages);

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

    let mut last_chunk = "content";

    let mut out = stdout();
    while let Some(Ok(chunk)) = response.next().await {
        match chunk {
            ChatChunk::Thinking(text) => {
                if last_chunk == "content" {
                    out.write_all("thinking:\n".as_bytes()).await.unwrap();
                    out.flush().await.unwrap();
                    last_chunk = "thinking";
                }

                out.write_all(text.as_bytes()).await.unwrap();
                out.flush().await.unwrap();
            }
            ChatChunk::Content(text) => {
                if last_chunk == "thinking" {
                    last_chunk = "content";
                }

                out.write_all(text.as_bytes()).await.unwrap();
                out.flush().await.unwrap();
            }
        }
    }
}

/// Collects all chunks in the response stream to a string.
async fn collect_response(mut response: ChatResponse<'_>) -> String {
    response.aggregate_lossy().await.content
}
