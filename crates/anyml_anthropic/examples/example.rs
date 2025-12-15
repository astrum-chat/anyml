use std::env;

use anyhttp_reqwest::ReqwestClientWrapper;
use anyml_anthropic::AnthropicProvider;
use anyml_core::{ChatOptions, ChatProvider, ChatResponse};

const MODEL: &'static str = "claude-3-haiku-20240307";
const STREAM_RESPONSE: bool = true;

struct Config {
    chat_provider: Box<dyn ChatProvider>,
}

#[tokio::main]
async fn main() {
    chat().await.unwrap();
}

async fn chat() -> anyhow::Result<()> {
    dotenvy::from_filename(".env.test").ok();
    let api_key = env::var("ANTHROPIC_API_KEY").expect("API_KEY not set");

    let config = Config {
        chat_provider: Box::new(AnthropicProvider::new(
            // We need to put the client in a wrapper
            // as a workaround to rust's orphan rule.
            ReqwestClientWrapper::new(reqwest::Client::new()),
            api_key,
        )),
    };

    let provider = &config.chat_provider;

    let messages = &["Write me a short poem!".into()];
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
    use futures::StreamExt;
    use tokio::io::{AsyncWriteExt, stdout};

    let mut out = stdout();
    while let Some(Ok(chunk)) = response.next().await {
        out.write_all(chunk.content.as_bytes()).await.unwrap();
        out.flush().await.unwrap();
    }
}

/// Collects all chunks in the response stream to a string.
async fn collect_response(response: ChatResponse<'_>) -> String {
    use futures::StreamExt;

    response
        .filter_map(async |this| this.ok())
        .map(|this| this.content)
        .collect()
        .await
}
