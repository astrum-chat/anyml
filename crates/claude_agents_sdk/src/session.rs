use std::fs;
use std::io::Write;
use std::path::PathBuf;

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::types::{Message, Role};
use crate::AgentError;

/// A fixed namespace UUID for generating deterministic v5 UUIDs from
/// arbitrary session ID strings. This ensures the same input always
/// produces the same UUID.
const SESSION_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30,
    0xc8,
]);

/// Normalize a session ID to a valid UUID string.
///
/// The Claude Code CLI requires session IDs to be valid UUIDs. This function:
/// - Returns the input as-is if it's already a valid UUID.
/// - Generates a deterministic UUID v5 from the input otherwise, so the same
///   string always maps to the same UUID.
pub fn normalize_session_id(id: &str) -> String {
    if Uuid::parse_str(id).is_ok() {
        id.to_string()
    } else {
        Uuid::new_v5(&SESSION_NAMESPACE, id.as_bytes()).to_string()
    }
}

/// Create a temporary session file from a list of messages.
///
/// Writes a JSONL file to the system temp directory and returns the full
/// path. The `.jsonl` extension tells the CLI to load messages directly
/// from the file (via `--resume <path>`), bypassing its internal session
/// directory lookup entirely.
///
/// If `id` is provided it is normalized to a UUID for the internal
/// `sessionId` field; otherwise a UUID v4 is generated.
pub fn create_session(
    messages: &[Message],
    id: Option<&str>,
) -> Result<String, AgentError> {
    let session_id = id
        .map(|s| normalize_session_id(s))
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let dir = session_dir();
    fs::create_dir_all(&dir).map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;

    let session_file = dir.join(format!("{session_id}.jsonl"));
    let mut file =
        fs::File::create(&session_file).map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;

    let cwd = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .to_string_lossy()
        .to_string();
    let now = Utc::now();
    let mut parent_uuid: Option<String> = None;

    for msg in messages {
        let uuid = Uuid::new_v4().to_string();
        let msg_type = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };

        let content = json!([{"type": "text", "text": msg.content}]);
        let line = json!({
            "type": msg_type,
            "message": {
                "role": msg_type,
                "content": content,
            },
            "uuid": uuid,
            "sessionId": session_id,
            "timestamp": now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "cwd": cwd,
            "version": "0.0.0",
            "gitBranch": "main",
            "userType": "external",
            "isSidechain": false,
            "parentUuid": parent_uuid,
        });

        serde_json::to_writer(&mut file, &line)
            .map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;
        file.write_all(b"\n")
            .map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;

        parent_uuid = Some(uuid);
    }

    Ok(session_file.to_string_lossy().to_string())
}

/// Returns the temp directory used to store session files.
fn session_dir() -> PathBuf {
    std::env::temp_dir().join("claude_sessions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_session_id_valid_uuid() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(normalize_session_id(uuid), uuid);
    }

    #[test]
    fn test_normalize_session_id_arbitrary_string() {
        let id = "12345";
        let normalized = normalize_session_id(id);
        assert!(Uuid::parse_str(&normalized).is_ok());
        assert_eq!(normalized, normalize_session_id(id));
        assert_ne!(normalized, id);
    }

    #[test]
    fn test_normalize_session_id_different_inputs_differ() {
        assert_ne!(normalize_session_id("abc"), normalize_session_id("xyz"));
    }

    #[test]
    fn test_create_session_returns_jsonl_path() {
        let messages = vec![
            Message { role: Role::User, content: "hello".into() },
            Message { role: Role::Assistant, content: "hi there".into() },
        ];

        let path = create_session(&messages, None).unwrap();
        assert!(path.ends_with(".jsonl"));
        assert!(std::path::Path::new(&path).exists());

        // Read and parse JSONL lines
        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let line0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(line0["type"], "user");
        assert_eq!(line0["message"]["role"], "user");
        assert_eq!(line0["message"]["content"][0]["text"], "hello");
        assert!(line0["parentUuid"].is_null());

        let line1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(line1["type"], "assistant");
        assert_eq!(line1["message"]["role"], "assistant");
        assert_eq!(line1["message"]["content"][0]["text"], "hi there");
        assert_eq!(line1["parentUuid"], line0["uuid"]);

        // Clean up
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_create_session_deterministic_path_with_id() {
        let messages = vec![Message { role: Role::User, content: "test".into() }];

        let path1 = create_session(&messages, Some("my-session")).unwrap();
        let path2 = create_session(&messages, Some("my-session")).unwrap();
        // Same id produces the same file path (deterministic UUID v5)
        assert_eq!(path1, path2);

        let _ = fs::remove_file(&path1);
    }
}
