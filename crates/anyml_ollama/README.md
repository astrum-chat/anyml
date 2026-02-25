# Anyml Ollama

An API wrapper for interacting with Ollama via Anyml.

Does not enforce a specific async runtime or http library via the [anyhttp](https://github.com/quaero-search/anyhttp) crate.

## Example usage
```rs
use anyml::{ChatOptions, ChatProvider};
use anyml_ollama::OllamaProvider;

let ollama = OllamaProvider::new(
    // We need to put the client in a wrapper
    // as a workaround to rust's orphan rule.
    ReqwestClientWrapper::new(reqwest::Client::new()),
);

let messages =  &[Message::user("Write me a short poem!")];
let options = ChatOptions::new("claude-3-haiku-20240307").messages(messages);

let response = ollama.chat(&options).await.unwrap();

use futures::StreamExt;

let response_msg = response
    .filter_map(async |this| this.ok())
    .map(|this| this.content)
    .collect()
    .await;
```
