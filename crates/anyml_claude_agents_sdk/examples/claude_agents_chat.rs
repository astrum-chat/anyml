use std::env;

use anyml_claude_agents_sdk::ClaudeAgentsProvider;
use anyml_core::{
    Message,
    providers::chat::{ChatChunk, ChatOptions, ChatProvider, ChatResponse},
};

const MODEL: &str = "claude-sonnet-4-6";

#[tokio::main]
async fn main() {
    chat().await.unwrap();
}

async fn chat() -> anyhow::Result<()> {
    dotenvy::from_filename(".env.test").ok();

    let mut provider = ClaudeAgentsProvider::new(which_claude()?);

    // If ANTHROPIC_SESSION is set, pass it; otherwise the CLI uses its stored credentials.
    if let Ok(session_key) = env::var("ANTHROPIC_SESSION") {
        provider = provider.api_key(session_key.into());
    }

    let messages = &[Message::user("Write me a short poem!")];
    // Any string works as a session_id — it's automatically normalized to a
    // valid UUID before being passed to the CLI.
    let options = ChatOptions::new(MODEL)
        .session_id("12345")
        .messages(messages);

    let response = provider.chat(&options).await.map_err(anyhow::Error::new)?;
    stream_response(response).await;

    Ok(())
}

fn which_claude() -> anyhow::Result<std::path::PathBuf> {
    which::which("claude").map_err(|e| anyhow::anyhow!("claude CLI not found on PATH: {e}"))
}

async fn stream_response(mut response: ChatResponse<'_>) {
    use tokio::io::{AsyncWriteExt, stdout};

    let mut out = stdout();
    while let Some(chunk) = response.next().await {
        match chunk {
            Ok(ChatChunk::Thinking(text)) => {
                out.write_all(b"\x1b[2m").await.unwrap();
                out.write_all(text.as_bytes()).await.unwrap();
                out.write_all(b"\x1b[0m").await.unwrap();
                out.flush().await.unwrap();
            }
            Ok(ChatChunk::Content(text)) => {
                out.write_all(text.as_bytes()).await.unwrap();
                out.flush().await.unwrap();
            }
            Err(e) => {
                eprintln!("stream error: {e}");
            }
        }
    }
    out.write_all(b"\n").await.unwrap();
}
