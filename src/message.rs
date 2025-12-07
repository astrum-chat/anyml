use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;

pub struct Message {
    pub content: String,
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

    pub fn as_json(
        &self,
        user_role: &str,
        assistant_role: &str,
        system_role: &str,
        tool_role: &str,
    ) -> Value {
        let role_str = match self.role {
            MessageRole::User => user_role,
            MessageRole::Assistant => assistant_role,
            MessageRole::System => system_role,
            MessageRole::Tool => tool_role,
        };

        json!({
            "role": role_str,
            "content": self.content,
        })
    }

    pub fn from_json(
        value: &Value,
        user_role: &str,
        assistant_role: &str,
        system_role: &str,
    ) -> Result<Self, ParseMessageError> {
        let role = value
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(|| ParseMessageError::MissingField("role"))?;

        let content = value
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| ParseMessageError::MissingField("content"))?
            .to_string();

        let role = if role == user_role {
            MessageRole::User
        } else if role == assistant_role {
            MessageRole::Assistant
        } else if role == system_role {
            MessageRole::System
        } else {
            return Err(ParseMessageError::InvalidValue {
                field: "role",
                value: role.to_string(),
            });
        };

        Ok(Self { content, role })
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

#[derive(Debug, Error)]
pub enum ParseMessageError {
    #[error("Missing field: {0}")]
    MissingField(&'static str),

    #[error("Invalid value for field `{field}`: {value}")]
    InvalidValue { field: &'static str, value: String },
}

#[derive(Serialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}
