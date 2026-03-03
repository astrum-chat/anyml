use serde::Deserialize;

/// A message received from the Claude Code CLI via NDJSON stdout.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum AgentMessage {
    #[serde(rename = "system")]
    System {
        subtype: String,
        session_id: Option<String>,
    },

    #[serde(rename = "assistant")]
    Assistant { message: AssistantContent },

    #[serde(rename = "result")]
    Result {
        subtype: String,
        result: Option<String>,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Debug, Deserialize)]
pub struct AssistantContent {
    pub content: Vec<ContentBlock>,
}

/// A content block within an assistant message.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "thinking")]
    Thinking { thinking: String },
}

/// An input message in the conversation history.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// The role of a message in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// Per-query configuration options.
#[derive(Debug, Default)]
pub struct QueryOptions {
    pub model: Option<String>,
    pub max_turns: Option<u32>,
    pub system_prompt: Option<String>,
    pub cwd: Option<std::path::PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_system_init() {
        let json = r#"{"type":"system","subtype":"init","session_id":"abc-123"}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::System {
                subtype,
                session_id,
            } => {
                assert_eq!(subtype, "init");
                assert_eq!(session_id.as_deref(), Some("abc-123"));
            }
            _ => panic!("expected System message"),
        }
    }

    #[test]
    fn parse_assistant_text() {
        let json = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello!"}]}}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::Assistant { message } => {
                assert_eq!(message.content.len(), 1);
                match &message.content[0] {
                    ContentBlock::Text { text } => assert_eq!(text, "Hello!"),
                    _ => panic!("expected Text block"),
                }
            }
            _ => panic!("expected Assistant message"),
        }
    }

    #[test]
    fn parse_assistant_thinking() {
        let json = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"Let me reason..."},{"type":"text","text":"The answer is 42."}]}}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::Assistant { message } => {
                assert_eq!(message.content.len(), 2);
                match &message.content[0] {
                    ContentBlock::Thinking { thinking } => {
                        assert_eq!(thinking, "Let me reason...")
                    }
                    _ => panic!("expected Thinking block"),
                }
                match &message.content[1] {
                    ContentBlock::Text { text } => assert_eq!(text, "The answer is 42."),
                    _ => panic!("expected Text block"),
                }
            }
            _ => panic!("expected Assistant message"),
        }
    }

    #[test]
    fn parse_result_success() {
        let json = r#"{"type":"result","subtype":"success","result":"Done.","is_error":false}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::Result {
                subtype,
                result,
                is_error,
            } => {
                assert_eq!(subtype, "success");
                assert_eq!(result.as_deref(), Some("Done."));
                assert!(!is_error);
            }
            _ => panic!("expected Result message"),
        }
    }

    #[test]
    fn parse_result_error() {
        let json = r#"{"type":"result","subtype":"error_during_execution","result":null,"is_error":true}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::Result {
                subtype,
                result,
                is_error,
            } => {
                assert_eq!(subtype, "error_during_execution");
                assert!(result.is_none());
                assert!(is_error);
            }
            _ => panic!("expected Result message"),
        }
    }

    #[test]
    fn parse_result_is_error_defaults_false() {
        let json = r#"{"type":"result","subtype":"success","result":"ok"}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::Result { is_error, .. } => assert!(!is_error),
            _ => panic!("expected Result message"),
        }
    }

    #[test]
    fn unknown_message_type_fails_parse() {
        let json = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"123","content":"ok"}]}}"#;
        let result = serde_json::from_str::<AgentMessage>(json);
        assert!(result.is_err());
    }
}

