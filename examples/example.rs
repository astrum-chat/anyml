use anyhttp_reqwest::ReqwestClientWrapper;
use anyml::{AnthropicProvider, ChatOptions, ChatProvider};
use futures::StreamExt;
use tokio::io::{AsyncWriteExt, stdout};

struct Config {
    chat_provider: Box<dyn ChatProvider>,
}

#[tokio::main]
async fn main() {
    let config = init_config().unwrap();

    let options = ChatOptions::new("claude-3-haiku-20240307");

    let response = config.chat_provider.chat(&options).await.unwrap();

    let mut out = stdout();
    while let Some(Ok(chunk)) = response.next().await {
        out.write_all(chunk.content.as_bytes()).await.unwrap();
        out.flush().await.unwrap();
    }
}

fn init_config() -> anyhow::Result<Config> {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("ANTHROPIC_API_KEY")?;

    let client = ReqwestClientWrapper::new(reqwest::Client::new());
    let anthropic = AnthropicProvider::new(client, api_key);

    Ok(Config {
        chat_provider: Box::new(anthropic),
    })
}
