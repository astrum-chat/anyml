use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message<C = String> {
    pub content: C,
    pub role: MessageRole,
}

impl Message {
    pub fn new(content: impl Into<String>, role: MessageRole) -> Self {
        Self {
            content: content.into(),
            role,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(content, MessageRole::User)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(content, MessageRole::Assistant)
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(content, MessageRole::System)
    }
}

impl<T> From<T> for Message
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        Message::user(value.into())
    }
}

#[derive(Debug, Clone)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
    Text,
    Unknown(String),
}

impl MessageRole {
    pub fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::Tool => "tool",
            Self::Text => "text",
            Self::Unknown(other) => other,
        }
    }

    pub fn from_str(str: &str) -> Self {
        match str {
            "user" => Self::User,
            "assistant" => Self::Assistant,
            "system" => Self::System,
            "tool" => Self::Tool,
            "text" => Self::Text,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

impl Serialize for MessageRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MessageRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.to_lowercase().as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            "tool" => MessageRole::Tool,
            "text" => MessageRole::Text,
            other => MessageRole::Unknown(other.to_string()),
        })
    }
}
