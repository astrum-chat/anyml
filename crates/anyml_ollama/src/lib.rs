use std::borrow::Cow;

use anyhttp::HttpClient;

mod chat;
mod list_models;

const DEFAULT_URL: &str = "http://localhost:11434";

pub struct OllamaProvider<C: HttpClient> {
    client: C,
    url: Cow<'static, str>,
}

impl<C: HttpClient> OllamaProvider<C> {
    pub fn new(client: C) -> Self {
        Self {
            client,
            url: Cow::Borrowed(DEFAULT_URL),
        }
    }

    pub fn url(mut self, url: impl Into<Cow<'static, str>>) -> Self {
        self.url = url.into();
        self
    }
}
