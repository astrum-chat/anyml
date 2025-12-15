use anyhttp_reqwest::ReqwestClientWrapper;
use anyml_core::{ChatOptions, ChatProvider, ChatResponse};
use anyml_ollama::OllamaProvider;

const MODEL: &'static str = "qwen2.5:1.5b";
const STREAM_RESPONSE: bool = true;

struct Config {
    chat_provider: Box<dyn ChatProvider>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config {
        chat_provider: Box::new(OllamaProvider::new(
            // We need to put the client in a wrapper
            // as a workaround to rust's orphan rule.
            ReqwestClientWrapper::new(reqwest::Client::new()),
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
