use std::path::PathBuf;

use claude_sdk::{
    AgentError, AgentMessage, ClaudeSDK, ContentBlock, Message, QueryOptions, Role,
    ThinkingConfig,
};
use secrecy::SecretString;

fn fake_cli() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("fake_cli.sh")
}

fn user_msg(content: &str) -> Message {
    Message { role: Role::User, content: content.into() }
}

fn assistant_msg(content: &str) -> Message {
    Message { role: Role::Assistant, content: content.into() }
}

// ── Streaming ────────────────────────────────────────────────────────

#[test]
fn stream_basic_messages() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_stream__")];
    let (stream, mut handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    // system init + assistant + result = 3 messages
    assert_eq!(results.len(), 3);

    // First: system init
    match results[0].as_ref().unwrap() {
        AgentMessage::System { subtype, session_id } => {
            assert_eq!(subtype, "init");
            assert_eq!(session_id.as_deref(), Some("test-session-1"));
        }
        other => panic!("expected System, got {other:?}"),
    }

    // Second: assistant text
    match results[1].as_ref().unwrap() {
        AgentMessage::Assistant { message } => {
            assert_eq!(message.content.len(), 1);
            match &message.content[0] {
                ContentBlock::Text { text } => assert_eq!(text, "Hello from fake CLI"),
                other => panic!("expected Text, got {other:?}"),
            }
        }
        other => panic!("expected Assistant, got {other:?}"),
    }

    // Third: result
    match results[2].as_ref().unwrap() {
        AgentMessage::Result { subtype, result, is_error } => {
            assert_eq!(subtype, "success");
            assert_eq!(result.as_deref(), Some("Done."));
            assert!(!is_error);
        }
        other => panic!("expected Result, got {other:?}"),
    }

    let _ = handle.wait();
}

#[test]
fn stream_thinking_blocks() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_thinking__")];
    let (stream, mut handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    // system + assistant + result
    assert_eq!(results.len(), 3);

    match results[1].as_ref().unwrap() {
        AgentMessage::Assistant { message } => {
            assert_eq!(message.content.len(), 2);
            match &message.content[0] {
                ContentBlock::Thinking { thinking } => assert_eq!(thinking, "Let me think..."),
                other => panic!("expected Thinking, got {other:?}"),
            }
            match &message.content[1] {
                ContentBlock::Text { text } => assert_eq!(text, "Answer"),
                other => panic!("expected Text, got {other:?}"),
            }
        }
        other => panic!("expected Assistant, got {other:?}"),
    }

    let _ = handle.wait();
}

#[test]
fn stream_skips_unknown_message_types() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_skip_unknown__")];
    let (stream, mut handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    // "user" type message should be silently skipped → system + assistant + result = 3
    assert_eq!(results.len(), 3);

    match results[1].as_ref().unwrap() {
        AgentMessage::Assistant { message } => {
            match &message.content[0] {
                ContentBlock::Text { text } => assert_eq!(text, "after unknown"),
                other => panic!("expected Text, got {other:?}"),
            }
        }
        other => panic!("expected Assistant, got {other:?}"),
    }

    let _ = handle.wait();
}

#[test]
fn stream_skips_empty_lines() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_empty_lines__")];
    let (stream, mut handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    // blank lines skipped → system + assistant + result = 3
    assert_eq!(results.len(), 3);

    match results[1].as_ref().unwrap() {
        AgentMessage::Assistant { message } => {
            match &message.content[0] {
                ContentBlock::Text { text } => assert_eq!(text, "survived blanks"),
                other => panic!("expected Text, got {other:?}"),
            }
        }
        other => panic!("expected Assistant, got {other:?}"),
    }

    let _ = handle.wait();
}

// ── Query options ────────────────────────────────────────────────────

#[test]
fn query_passes_model_flag() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_args__")];
    let options = QueryOptions {
        model: Some("test-model".into()),
        ..Default::default()
    };
    let (stream, mut handle) = sdk.query(&messages, &options).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(result_text.contains("--model"), "args: {result_text}");
    assert!(result_text.contains("test-model"), "args: {result_text}");

    let _ = handle.wait();
}

#[test]
fn query_passes_max_turns_flag() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_args__")];
    let options = QueryOptions {
        max_turns: Some(5),
        ..Default::default()
    };
    let (stream, mut handle) = sdk.query(&messages, &options).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(result_text.contains("--max-turns"), "args: {result_text}");
    assert!(result_text.contains("5"), "args: {result_text}");

    let _ = handle.wait();
}

#[test]
fn query_passes_system_prompt_flag() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_args__")];
    let options = QueryOptions {
        system_prompt: Some("Be helpful".into()),
        ..Default::default()
    };
    let (stream, mut handle) = sdk.query(&messages, &options).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(result_text.contains("--system-prompt"), "args: {result_text}");
    assert!(result_text.contains("Be helpful"), "args: {result_text}");

    let _ = handle.wait();
}

#[test]
fn query_passes_api_key_env() {
    let sdk = ClaudeSDK::new(fake_cli())
        .api_key(SecretString::from("sk-ant-api03-fake-key"));
    let messages = [user_msg("__test_echo_env__")];
    let (stream, mut handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(result_text.contains("API_KEY: sk-ant-api03-fake-key"), "env: {result_text}");
    assert!(result_text.contains("OAUTH_TOKEN: unset"), "env: {result_text}");

    let _ = handle.wait();
}

#[test]
fn query_passes_session_token_as_oauth() {
    let sdk = ClaudeSDK::new(fake_cli())
        .api_key(SecretString::from("sk-ant-sid01-fake-session-token"));
    let messages = [user_msg("__test_echo_env__")];
    let (stream, mut handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    // Session tokens should go to CLAUDE_CODE_OAUTH_TOKEN, not ANTHROPIC_API_KEY
    assert!(result_text.contains("API_KEY: unset"), "env: {result_text}");
    assert!(result_text.contains("OAUTH_TOKEN: sk-ant-sid01-fake-session-token"), "env: {result_text}");

    let _ = handle.wait();
}

// ── Thinking options ─────────────────────────────────────────────────

#[test]
fn query_passes_thinking_budget_env() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_args_env__")];
    let options = QueryOptions {
        thinking: Some(ThinkingConfig::BudgetTokens(8192)),
        ..Default::default()
    };
    let (stream, mut handle) = sdk.query(&messages, &options).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream).collect();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(result_text.contains("MAX_THINKING_TOKENS: 8192"), "output: {result_text}");

    let _ = handle.wait();
}

#[test]
fn query_passes_thinking_effort_env() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_args_env__")];
    let options = QueryOptions {
        thinking: Some(ThinkingConfig::Effort("high".into())),
        ..Default::default()
    };
    let (stream, mut handle) = sdk.query(&messages, &options).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream).collect();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(result_text.contains("MAX_THINKING_TOKENS: 32768"), "output: {result_text}");

    let _ = handle.wait();
}

#[test]
fn query_passes_thinking_disabled_env() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_args_env__")];
    let options = QueryOptions {
        thinking: Some(ThinkingConfig::Disabled),
        ..Default::default()
    };
    let (stream, mut handle) = sdk.query(&messages, &options).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream).collect();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(result_text.contains("MAX_THINKING_TOKENS: 0"), "output: {result_text}");

    let _ = handle.wait();
}

#[test]
fn query_no_thinking_config_disables_thinking() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_args_env__")];
    let options = QueryOptions::default();
    let (stream, mut handle) = sdk.query(&messages, &options).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream).collect();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(result_text.contains("MAX_THINKING_TOKENS: 0"), "output: {result_text}");

    let _ = handle.wait();
}

// ── Error cases ──────────────────────────────────────────────────────

#[test]
fn query_no_user_message_errors() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [assistant_msg("hi")];
    match sdk.query(&messages, &QueryOptions::default()) {
        Err(AgentError::NoUserMessage) => {}
        Err(other) => panic!("expected NoUserMessage, got {other:?}"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}

#[test]
fn query_empty_messages_errors() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages: Vec<Message> = vec![];
    match sdk.query(&messages, &QueryOptions::default()) {
        Err(AgentError::NoUserMessage) => {}
        Err(other) => panic!("expected NoUserMessage, got {other:?}"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}

// ── Handle lifecycle ─────────────────────────────────────────────────

#[test]
fn handle_kill_terminates_process() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_hang__")];
    let (_stream, mut handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    // Process should be running; kill it
    handle.kill().unwrap();
    let status = handle.wait().unwrap();
    // On unix, killed processes don't have success status
    assert!(!status.success());
}

#[test]
fn handle_drop_kills_process() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_hang__")];
    let (_stream, handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    // Drop the handle — should kill the process without panicking
    drop(handle);
}

// ── Stdin message piping ─────────────────────────────────────────────

#[test]
fn query_sends_single_message_via_stdin() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_stdin__")];
    let (stream, mut handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    // Single message should be sent as-is (no role labels) in a user message
    assert!(result_text.contains("STDIN:"), "output: {result_text}");
    assert!(result_text.contains("__test_echo_stdin__"), "output: {result_text}");
    assert!(result_text.contains(r#""role":"user""#), "output: {result_text}");

    let _ = handle.wait();
}

#[test]
fn query_sends_only_last_user_message_via_stdin() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [
        user_msg("hello"),
        assistant_msg("hi there"),
        user_msg("__test_echo_stdin__"),
    ];
    let (stream, mut handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    // Only the last user message is sent via stdin (history is managed via sessions)
    assert!(result_text.contains("STDIN:"), "output: {result_text}");
    assert!(result_text.contains("__test_echo_stdin__"), "output: {result_text}");
    // History should NOT appear in stdin
    assert!(!result_text.contains("hello|"), "history should not be in stdin: {result_text}");

    let _ = handle.wait();
}

// ── Session support ──────────────────────────────────────────────────

#[test]
fn query_passes_resume_flag() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_args__")];
    let options = QueryOptions {
        session_id: Some("test-session-abc".into()),
        ..Default::default()
    };
    let (stream, mut handle) = sdk.query(&messages, &options).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(result_text.contains("--resume"), "args: {result_text}");
    assert!(result_text.contains("test-session-abc"), "args: {result_text}");

    let _ = handle.wait();
}

#[test]
fn query_no_session_id_omits_resume() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_args__")];
    let options = QueryOptions::default();
    let (stream, mut handle) = sdk.query(&messages, &options).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(!result_text.contains("--resume"), "args should not contain --resume: {result_text}");

    let _ = handle.wait();
}

// ── Default flags always present ─────────────────────────────────────

#[test]
fn query_always_passes_default_flags() {
    let sdk = ClaudeSDK::new(fake_cli());
    let messages = [user_msg("__test_echo_args__")];
    let (stream, mut handle) = sdk.query(&messages, &QueryOptions::default()).unwrap();

    let results: Vec<_> = futures::executor::block_on_stream(stream)
        .collect::<Vec<_>>();

    let result_text = match results.last().unwrap().as_ref().unwrap() {
        AgentMessage::Result { result, .. } => result.as_deref().unwrap(),
        other => panic!("expected Result, got {other:?}"),
    };

    assert!(result_text.contains("--output-format"), "args: {result_text}");
    assert!(result_text.contains("--input-format"), "args: {result_text}");
    assert!(result_text.contains("stream-json"), "args: {result_text}");
    assert!(result_text.contains("--print"), "args: {result_text}");
    assert!(result_text.contains("--tools"), "args: {result_text}");

    let _ = handle.wait();
}
