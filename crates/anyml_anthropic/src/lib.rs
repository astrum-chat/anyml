use anyhttp::HttpClient;
use secrecy::SecretString;
use std::borrow::Cow;

mod chat;
mod list_models;

const DEFAULT_URL: &str = "https://api.anthropic.com";

pub struct AnthropicProvider<C: HttpClient> {
    client: C,
    url: Cow<'static, str>,
    api_key: SecretString,
}

impl<C: HttpClient> AnthropicProvider<C> {
    pub fn new(client: C, api_key: impl Into<SecretString>) -> Self {
        Self {
            client,
            url: Cow::Borrowed(DEFAULT_URL),
            api_key: api_key.into(),
        }
    }

    pub fn url(mut self, url: impl Into<Cow<'static, str>>) -> Self {
        self.url = url.into();
        self
    }

    pub fn api_key(mut self, api_key: impl Into<SecretString>) -> Self {
        self.api_key = api_key.into();
        self
    }
}
