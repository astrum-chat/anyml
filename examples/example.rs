use anyml::{AnthropicProvider, ChatChunk, Message};
use anyml_core::providers::chat::{ChatOptions, ChatProvider};
use tokio::io::{AsyncWriteExt, stdout};

struct Config {
    chat_provider: Box<dyn ChatProvider>,
}

#[tokio::main]
async fn main() {
    let config = init_config().unwrap();

    let messages = &[Message::user("Write me a short poem!")];
    let options = ChatOptions::new("claude-3-haiku-20240307").messages(messages);

    let mut response = config.chat_provider.chat(&options).await.unwrap();

    let mut out = stdout();
    while let Some(Ok(chunk)) = response.next().await {
        if let ChatChunk::Content(text) = chunk {
            out.write_all(text.as_bytes()).await.unwrap();
            out.flush().await.unwrap();
        }
    }
}

fn init_config() -> anyhow::Result<Config> {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("ANTHROPIC_API_KEY")?;

    let anthropic = AnthropicProvider::new(reqwest::Client::new(), api_key);

    Ok(Config {
        chat_provider: Box::new(anthropic),
    })
}
