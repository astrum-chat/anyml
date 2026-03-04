use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::anyhow;
use anyml_core::providers::chat::{
    ChatChunk, ChatError, ChatOptions, ChatProvider, ChatResponse, ChatStreamError, Messages,
    Thinking,
};
use claude_agents_sdk::{
    AgentError, AgentHandle, AgentMessage, Message, QueryOptions, Role, StreamDelta, StreamEvent,
    ThinkingConfig, create_session, normalize_session_id,
};
use futures::{Stream, StreamExt};

use crate::ClaudeAgentsProvider;

#[async_trait::async_trait]
impl ChatProvider for ClaudeAgentsProvider {
    async fn chat(&self, options: &ChatOptions<'_>) -> Result<ChatResponse, ChatError> {
        let (messages, system_prompt) = convert_messages(&options.messages)?;

        let cwd = std::env::current_dir().ok();

        // When a session_id is provided and there is conversation history
        // (more than just the last user message), write a session file so
        // the CLI can resume with full context.
        // Normalize the session ID to a valid UUID (the CLI requires UUID format).
        // Only resume a session when there's actual history to restore.
        // On the first message, no session file exists yet so --resume would fail.
        let session_id = match options.session_id {
            Some(sid) if messages.len() > 1 => {
                let normalized = normalize_session_id(sid);
                let project_path = cwd.as_deref().unwrap_or_else(|| std::path::Path::new("/tmp"));
                let last_user_idx = messages
                    .iter()
                    .rposition(|m| m.role == Role::User)
                    .unwrap_or(messages.len() - 1);
                let history = &messages[..last_user_idx];
                if !history.is_empty() {
                    create_session(project_path, history, Some(&normalized))
                        .map_err(|e| ChatError::RequestError(anyhow!(e)))?;
                    Some(normalized)
                } else {
                    None
                }
            }
            _ => None,
        };

        let mut query_options = QueryOptions {
            model: Some(options.model.to_owned()),
            system_prompt,
            session_id,
            thinking: options.thinking.as_ref().map(|t| match t {
                Thinking::BudgetTokens(n) => ThinkingConfig::BudgetTokens(*n),
                Thinking::Effort(level) => ThinkingConfig::Effort(level.clone()),
                Thinking::Enabled => ThinkingConfig::BudgetTokens(10000),
            }),
            ..Default::default()
        };

        if let Some(cwd) = cwd {
            query_options.cwd = Some(cwd);
        }

        let (stream, handle) = self
            .sdk
            .query(&messages, &query_options)
            .map_err(|e| ChatError::RequestError(anyhow!(e)))?;

        let chunk_stream = stream.filter_map(|msg| async {
            match msg {
                Ok(AgentMessage::StreamEvent {
                    event: StreamEvent::ContentBlockDelta { delta },
                }) => match delta {
                    StreamDelta::Text { text } => Some(Ok(ChatChunk::Content(text))),
                    StreamDelta::Thinking { thinking } => Some(Ok(ChatChunk::Thinking(thinking))),
                    StreamDelta::Other => None,
                },
                Ok(AgentMessage::Result {
                    is_error: true,
                    result,
                    subtype,
                }) => {
                    let msg = match result {
                        Some(r) => format!("CLI error ({subtype}): {r}"),
                        None => format!("CLI error ({subtype}): no details"),
                    };
                    Some(Err(ChatStreamError::ParseError(anyhow!(msg))))
                }
                Ok(
                    AgentMessage::Assistant { .. }
                    | AgentMessage::System { .. }
                    | AgentMessage::Result { .. }
                    | AgentMessage::StreamEvent { .. },
                ) => None,
                Err(AgentError::Io(e)) => Some(Err(ChatStreamError::ParseError(anyhow!(e)))),
                Err(e) => Some(Err(ChatStreamError::ParseError(anyhow!("{e}")))),
            }
        });

        Ok(ChatResponse::new(HandleStream {
            inner: Box::pin(chunk_stream),
            _handle: handle,
        }))
    }
}

/// Keeps the [`AgentHandle`] alive for the lifetime of the stream.
/// When the stream is dropped, the handle is dropped, killing the child process.
struct HandleStream<I> {
    inner: Pin<Box<dyn Stream<Item = I> + Send>>,
    _handle: AgentHandle,
}

impl<I> Stream for HandleStream<I> {
    type Item = I;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

fn convert_messages(messages: &Messages<'_>) -> Result<(Vec<Message>, Option<String>), ChatError> {
    let core_messages: Vec<anyml_core::Message> = match messages {
        Messages::Raw(msgs) => msgs.to_vec(),
        Messages::Serialized(raw) => serde_json::from_str(raw.get())
            .map_err(|e| ChatError::RequestBuildFailed(anyhow!(e)))?,
    };

    let mut sdk_messages = Vec::new();
    let mut system_prompt = None;

    for msg in core_messages {
        match msg.role {
            anyml_core::MessageRole::System => {
                let sp = system_prompt.get_or_insert_with(String::new);
                if !sp.is_empty() {
                    sp.push('\n');
                }
                sp.push_str(&msg.content);
            }
            anyml_core::MessageRole::User => {
                sdk_messages.push(Message {
                    role: Role::User,
                    content: msg.content,
                });
            }
            anyml_core::MessageRole::Assistant => {
                sdk_messages.push(Message {
                    role: Role::Assistant,
                    content: msg.content,
                });
            }
            _ => {
                sdk_messages.push(Message {
                    role: Role::User,
                    content: msg.content,
                });
            }
        }
    }

    Ok((sdk_messages, system_prompt))
}
