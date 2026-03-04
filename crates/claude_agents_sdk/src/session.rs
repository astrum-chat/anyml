use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

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

/// Returns the session storage directory for a given project path.
///
/// Claude Code stores sessions at `~/.claude/projects/{encoded_path}/`.
/// The path is encoded by replacing `/` with `-` (and stripping the leading slash).
pub fn session_dir(project_path: &Path) -> PathBuf {
    let home = dirs::home_dir().expect("could not determine home directory");
    let encoded = encode_path(project_path);
    home.join(".claude").join("projects").join(encoded)
}

/// Create a session file from a list of messages.
///
/// If `id` is provided it is used as the session ID; otherwise a UUID v4 is
/// generated. Writes a JSONL session file and updates `sessions-index.json`.
/// Returns the session ID.
pub fn create_session(
    project_path: &Path,
    messages: &[Message],
    id: Option<&str>,
) -> Result<String, AgentError> {
    let session_id = id
        .map(|s| normalize_session_id(s))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let dir = session_dir(project_path);
    fs::create_dir_all(&dir).map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;

    let session_file = dir.join(format!("{session_id}.jsonl"));
    let mut file =
        fs::File::create(&session_file).map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;

    let cwd = project_path.to_string_lossy().to_string();
    let now = Utc::now();
    let mut parent_uuid: Option<String> = None;

    for msg in messages {
        let uuid = Uuid::new_v4().to_string();
        let msg_type = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        let role = msg_type;

        let content = json!([{"type": "text", "text": msg.content}]);
        let line = json!({
            "type": msg_type,
            "message": {
                "role": role,
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

    // Update sessions-index.json
    update_sessions_index(&dir, &session_id, &session_file, project_path, &now)?;

    Ok(session_id)
}

/// Encode a filesystem path to the format Claude Code uses for project directories.
///
/// The CLI replaces every `/` with `-` (keeping the leading `-` that results
/// from the root `/`) and also replaces underscores `_` with `-`.
fn encode_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    s.replace('/', "-").replace('_', "-")
}

fn update_sessions_index(
    dir: &Path,
    session_id: &str,
    session_file: &Path,
    project_path: &Path,
    timestamp: &chrono::DateTime<Utc>,
) -> Result<(), AgentError> {
    let index_path = dir.join("sessions-index.json");

    let mut index: serde_json::Value = if index_path.exists() {
        let content =
            fs::read_to_string(&index_path).map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;
        serde_json::from_str(&content).unwrap_or_else(|_| default_index(project_path))
    } else {
        default_index(project_path)
    };

    let ts = timestamp.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let entry = json!({
        "sessionId": session_id,
        "fullPath": session_file.to_string_lossy(),
        "created": ts,
        "modified": ts,
        "messageCount": 0,
        "firstPrompt": "",
        "gitBranch": "main",
        "projectPath": project_path.to_string_lossy(),
        "isSidechain": false,
    });

    if let Some(entries) = index.get_mut("entries").and_then(|e| e.as_array_mut()) {
        entries.push(entry);
    }

    let content =
        serde_json::to_string_pretty(&index).map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;
    fs::write(&index_path, content).map_err(|e| AgentError::Io(anyhow::anyhow!(e)))?;

    Ok(())
}

fn default_index(project_path: &Path) -> serde_json::Value {
    json!({
        "version": 1,
        "entries": [],
        "originalPath": project_path.to_string_lossy(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_path() {
        let path = Path::new("/Volumes/T7/foo/bar");
        assert_eq!(encode_path(path), "-Volumes-T7-foo-bar");
    }

    #[test]
    fn test_encode_path_underscores() {
        let path = Path::new("/Volumes/T7/astrum_chat_main/anyml");
        assert_eq!(encode_path(path), "-Volumes-T7-astrum-chat-main-anyml");
    }

    #[test]
    fn test_encode_path_no_leading_slash() {
        let path = Path::new("relative/path");
        assert_eq!(encode_path(path), "relative-path");
    }

    #[test]
    fn test_session_dir_structure() {
        let path = Path::new("/Volumes/T7/my-project");
        let dir = session_dir(path);
        assert!(dir.to_string_lossy().contains(".claude/projects/-Volumes-T7-my-project"));
    }

    #[test]
    fn test_create_session_writes_jsonl() {
        let tmp = tempdir();
        let project_path = tmp.path();

        let messages = vec![
            Message { role: Role::User, content: "hello".into() },
            Message { role: Role::Assistant, content: "hi there".into() },
        ];

        let session_id = create_session(project_path, &messages, None).unwrap();

        // Verify session file exists
        let dir = session_dir(project_path);
        let session_file = dir.join(format!("{session_id}.jsonl"));
        assert!(session_file.exists());

        // Read and parse JSONL lines
        let content = fs::read_to_string(&session_file).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let line0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(line0["type"], "user");
        assert_eq!(line0["message"]["role"], "user");
        assert_eq!(line0["message"]["content"][0]["text"], "hello");
        assert_eq!(line0["sessionId"], session_id);
        assert!(line0["parentUuid"].is_null());

        let line1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(line1["type"], "assistant");
        assert_eq!(line1["message"]["role"], "assistant");
        assert_eq!(line1["message"]["content"][0]["text"], "hi there");
        assert_eq!(line1["sessionId"], session_id);
        // parentUuid should be the uuid of the first message
        assert_eq!(line1["parentUuid"], line0["uuid"]);
    }

    #[test]
    fn test_create_session_updates_index() {
        let tmp = tempdir();
        let project_path = tmp.path();

        let messages = vec![Message { role: Role::User, content: "test".into() }];
        let session_id = create_session(project_path, &messages, None).unwrap();

        let dir = session_dir(project_path);
        let index_path = dir.join("sessions-index.json");
        assert!(index_path.exists());

        let content = fs::read_to_string(&index_path).unwrap();
        let index: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(index["version"], 1);

        let entries = index["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["sessionId"], session_id);
    }

    #[test]
    fn test_normalize_session_id_valid_uuid() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(normalize_session_id(uuid), uuid);
    }

    #[test]
    fn test_normalize_session_id_arbitrary_string() {
        let id = "12345";
        let normalized = normalize_session_id(id);
        // Should be a valid UUID
        assert!(Uuid::parse_str(&normalized).is_ok());
        // Should be deterministic
        assert_eq!(normalized, normalize_session_id(id));
        // Should differ from the input
        assert_ne!(normalized, id);
    }

    #[test]
    fn test_normalize_session_id_different_inputs_differ() {
        assert_ne!(normalize_session_id("abc"), normalize_session_id("xyz"));
    }

    /// Create a temporary directory for testing that uses the home dir's .claude path.
    /// We override session_dir by using a temp dir as the "project path".
    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
}
