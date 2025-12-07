# AnyAi Ollama

An API wrapper for interacting with Ollama via AnyAi.

## Example usage
```rs
use anyai::{ChatOptions, ChatProvider};
use anyai_ollama::OllamaProvider;

let ollama = OllamaProvider::new(
    "http://localhost:11434",
    reqwest::Client::new()
);

let messages = &["Write me a short poem!".into()];
let options = ChatOptions::new("qwen2.5:1.5b").messages(messages);

let response = ollama.chat(&options).await.unwrap();

use futures::StreamExt;

let response_msg = response
    .filter_map(async |this| this.ok())
    .collect::<String>()
    .await;
```
