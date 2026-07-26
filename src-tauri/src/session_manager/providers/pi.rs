//! Pi (`pi` CLI) session scanner.
//!
//! Layout (v3, observed `@earendil-works/pi-coding-agent` >= 0.52):
//! - Root: `~/.pi/agent/sessions/` (override: `settings.json sessionDir`,
//!   then `PI_CODING_AGENT_SESSION_DIR`) — resolved by [`crate::pi_config`].
//! - Per-working-directory bucket: `--<cwd-mangled>--/` (`/` → `-`).
//! - Session file: `<rfc3339-ish-ts>_<uuid>.jsonl`, append-only JSONL.
//!   First line is the header:
//!   `{"type":"session","version":3,"id":"<uuid>","timestamp":"...","cwd":"..."}`
//!   followed by entries
//!   `{"type":"message",...,"message":{"role":"user"|"assistant"|"toolResult",...}}`
//!   and control entries (`model_change`, `thinking_level_change`, ...).
//!
//! Read-only: scanning/loading never writes to Pi files. Delete only fires on
//! explicit user request from the session manager UI.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{extract_text, parse_timestamp_to_ms, truncate_summary, TITLE_MAX_CHARS};

const PROVIDER_ID: &str = "pi";

pub fn session_roots() -> Vec<PathBuf> {
    vec![crate::pi_config::get_pi_sessions_dir()]
}

pub fn scan_sessions() -> Vec<SessionMeta> {
    let mut sessions = Vec::new();
    for root in session_roots() {
        if !root.exists() {
            continue;
        }
        let Ok(bucket_entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for bucket in bucket_entries.flatten() {
            let bucket_path = bucket.path();
            if !bucket_path.is_dir() {
                continue;
            }
            let Ok(file_entries) = std::fs::read_dir(&bucket_path) else {
                continue;
            };
            for file in file_entries.flatten() {
                let path = file.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Some(meta) = probe_session_file(&path) {
                    sessions.push(meta);
                }
            }
        }
    }
    sessions
}

/// Build a [`SessionMeta`] from one session file, or `None` when the file has
/// no v3-style header (not a Pi session log).
fn probe_session_file(path: &Path) -> Option<SessionMeta> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);

    let mut session_id: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut created: Option<i64> = None;
    let mut updated: Option<i64> = None;
    let mut first_user: Option<String> = None;
    let mut saw_header = false;

    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let typ = value.get("type").and_then(Value::as_str).unwrap_or("");

        if !saw_header {
            if typ != "session" {
                // Not a Pi session file (or corrupt head) — bail out entirely.
                return None;
            }
            saw_header = true;
            session_id = value
                .get("id")
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            cwd = value
                .get("cwd")
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            created = value.get("timestamp").and_then(parse_timestamp_to_ms);
            continue;
        }

        if let Some(ts) = value.get("timestamp").and_then(parse_timestamp_to_ms) {
            updated = Some(ts);
        }

        if first_user.is_none() && typ == "message" {
            let role = value
                .get("message")
                .and_then(|m| m.get("role"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if role == "user" {
                let text = value
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .map(extract_text)
                    .unwrap_or_default();
                if !text.trim().is_empty() {
                    first_user = Some(truncate_summary(&text, TITLE_MAX_CHARS));
                }
            }
        }
    }

    if !saw_header {
        return None;
    }

    // Fall back to the `<ts>_<uuid>.jsonl` filename for the id.
    let session_id = session_id
        .filter(|s| !s.is_empty())
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .and_then(|stem| stem.rsplit('_').next())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    let title = first_user.clone();
    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id: session_id.clone(),
        title,
        summary: first_user,
        project_dir: cwd,
        created_at: created,
        last_active_at: updated.or(created),
        source_path: Some(path.display().to_string()),
        resume_command: Some(format!("pi --session {session_id}")),
    })
}

/// Delete a Pi session file (only ever invoked on explicit user request).
pub fn delete_session(_root: &Path, source_path: &Path, _session_id: &str) -> Result<bool, String> {
    if !source_path.exists() {
        return Ok(false);
    }
    std::fs::remove_file(source_path)
        .map_err(|e| format!("Failed to delete Pi session {}: {e}", source_path.display()))?;
    Ok(true)
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open Pi session log: {e}"))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        let role = match message.get("role").and_then(Value::as_str).unwrap_or("") {
            "user" => "user",
            "assistant" => "assistant",
            "toolResult" => "tool",
            _ => continue,
        };
        let content = message
            .get("content")
            .map(extract_pi_content_text)
            .unwrap_or_default();
        if content.trim().is_empty() && role != "tool" {
            continue;
        }
        // Entry-level RFC3339 timestamp wins; message-level epoch ms is the fallback.
        let ts = value
            .get("timestamp")
            .and_then(parse_timestamp_to_ms)
            .or_else(|| message.get("timestamp").and_then(parse_timestamp_to_ms));

        messages.push(SessionMessage {
            role: role.to_string(),
            content,
            ts,
        });
    }

    Ok(messages)
}

/// Extract displayable text from Pi message content parts.
///
/// Unlike Claude's `tool_use`, Pi names tool invocations `toolCall`; surface
/// them as `[Tool: name]` markers. `thinking` parts are skipped (consistent
/// with the other providers, which never display reasoning blobs).
fn extract_pi_content_text(content: &Value) -> String {
    match content {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
                if item_type == "toolCall" {
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    return Some(format!("[Tool: {name}]"));
                }
                if item_type == "thinking" {
                    return None;
                }
                let text = extract_text(item);
                if text.trim().is_empty() {
                    None
                } else {
                    Some(text)
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => extract_text(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const HEADER: &str = r#"{"type":"session","version":3,"id":"019f979f-2ea8-7e64-a5d8-6248a0848568","timestamp":"2026-07-25T04:53:39.624Z","cwd":"/tmp/project"}"#;

    fn write_session(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let bucket = dir.join("--tmp-project--");
        std::fs::create_dir_all(&bucket).unwrap();
        let path = bucket.join(name);
        let mut f = File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        path
    }

    #[test]
    fn probe_extracts_header_first_user_and_last_ts() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(
            dir.path(),
            "2026-07-25T04-53-39-624Z_019f979f-2ea8-7e64-a5d8-6248a0848568.jsonl",
            &[
                HEADER,
                r#"{"type":"model_change","id":"568f71d3","parentId":null,"timestamp":"2026-07-25T04:53:39.930Z","provider":"kimi-coding","modelId":"k3-256k"}"#,
                r#"{"type":"message","id":"694904fe","parentId":"306be0bf","timestamp":"2026-07-25T04:55:27.834Z","message":{"role":"user","content":[{"type":"text","text":"add a feature please"}],"timestamp":1784955327824}}"#,
                r#"{"type":"message","id":"666c87cd","parentId":"694904fe","timestamp":"2026-07-25T04:55:36.023Z","message":{"role":"assistant","content":[{"type":"text","text":"on it"}],"timestamp":1784955336023}}"#,
            ],
        );

        let meta = probe_session_file(&path).expect("meta");
        assert_eq!(meta.provider_id, "pi");
        assert_eq!(meta.session_id, "019f979f-2ea8-7e64-a5d8-6248a0848568");
        assert_eq!(meta.project_dir.as_deref(), Some("/tmp/project"));
        assert_eq!(meta.title.as_deref(), Some("add a feature please"));
        assert_eq!(
            meta.created_at,
            parse_timestamp_to_ms(&Value::String("2026-07-25T04:53:39.624Z".into()))
        );
        assert!(
            meta.last_active_at.unwrap() > meta.created_at.unwrap(),
            "last_active must come from the newest entry"
        );
        assert_eq!(
            meta.resume_command.as_deref(),
            Some("pi --session 019f979f-2ea8-7e64-a5d8-6248a0848568")
        );
    }

    #[test]
    fn probe_rejects_files_without_session_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(
            dir.path(),
            "x_019f979f-0000-0000-0000-000000000000.jsonl",
            &[
                r#"{"type":"message","id":"a","timestamp":"2026-07-25T04:55:27.834Z","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#,
            ],
        );
        assert!(probe_session_file(&path).is_none());
    }

    #[test]
    fn load_messages_maps_roles_and_skips_thinking() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(
            dir.path(),
            "t_019f979f-0000-0000-0000-000000000000.jsonl",
            &[
                HEADER,
                r#"{"type":"message","id":"u1","timestamp":"2026-07-25T04:55:27.834Z","message":{"role":"user","content":[{"type":"text","text":"hello pi"}],"timestamp":1784955327824}}"#,
                r#"{"type":"message","id":"a1","timestamp":"2026-07-25T04:55:36.023Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"secret"},{"type":"text","text":"hi there"},{"type":"toolCall","name":"bash","arguments":{}}],"timestamp":1784955336023}}"#,
                r#"{"type":"message","id":"t1","timestamp":"2026-07-25T04:55:40.000Z","message":{"role":"toolResult","toolCallId":"x","toolName":"bash","content":[{"type":"text","text":"total 3"}],"isError":false,"timestamp":1784955340000}}"#,
            ],
        );

        let messages = load_messages(&path).expect("messages");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "hello pi");
        assert_eq!(messages[0].ts, Some(1784955327834));
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "hi there\n[Tool: bash]");
        assert_eq!(messages[2].role, "tool");
        assert_eq!(messages[2].content, "total 3");
    }

    #[test]
    fn delete_session_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path(), "d_deadbeef.jsonl", &[HEADER]);
        assert!(delete_session(dir.path(), &path, "deadbeef").unwrap());
        assert!(!path.exists());
        // Second delete reports not-found (idempotent, not an error).
        assert!(!delete_session(dir.path(), &path, "deadbeef").unwrap());
    }
}
