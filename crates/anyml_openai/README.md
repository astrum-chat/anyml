# AnyML OpenAI

An API wrapper for interacting with OpenAI via AnyML.

Does not enforce a specific async runtime or http library via the [anyhttp](https://github.com/quaero-search/anyhttp) crate.

## Example usage
```rs
use anyml::{ChatOptions, ChatProvider};
use anyml_openai::OpenAiProvider;

let api_key = std::env::var("OPENAI_API_KEY")
    .expect("OPENAI_API_KEY not set");

let openai = OpenAiProvider::new(
    // We need to put the client in a wrapper
    // as a workaround to rust's orphan rule.
    ReqwestClientWrapper::new(reqwest::Client::new()),
    api_key
);

let messages = &["Write me a short poem!".into()];
let options = ChatOptions::new("gpt-5-nano").messages(messages);

let response = openai.chat(&options).await.unwrap();

use futures::StreamExt;

let response_msg = response
    .filter_map(async |this| this.ok())
    .map(|this| this.content)
    .collect()
    .await;
```
