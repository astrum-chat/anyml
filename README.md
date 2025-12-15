# Anyml

Dyn-compatible traits which provide a unified API for asynchronously interacting with machine learning inference providers.

Crates for different providers can be found here:

- [anyml_anthropic](crates/anyml_anthropic)
- [anyml_ollama](crates/anyml_ollama)
- [anyml_openai](crates/anyml_openai)

## Installation
```toml
anyml = { git = "https://github.com/astrum-chat/anyml", features = ["anthropic", "ollama", "openai"] }
```

## Example

```rs
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

    let mut response = config.chat_provider.chat(&options).await.unwrap();

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

```
