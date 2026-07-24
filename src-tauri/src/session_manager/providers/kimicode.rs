//! Kimi Code CLI session scanner.
//!
//! Layout:
//! - Index: `~/.kimi-code/session_index.jsonl`
//!   `{ "sessionId", "sessionDir", "workDir" }`
//! - Messages: `<sessionDir>/agents/main/wire.jsonl`
//!   lines of `{ "type": "user.message" | "assistant.message" | ..., ... }`

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::kimi_code_config::{get_kimi_code_session_index_path, get_kimi_code_sessions_dir};
use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{extract_text, parse_timestamp_to_ms, truncate_summary, TITLE_MAX_CHARS};

const PROVIDER_ID: &str = "kimicode";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexEntry {
    session_id: String,
    session_dir: String,
    #[serde(default)]
    work_dir: Option<String>,
}

pub fn session_roots() -> Vec<PathBuf> {
    vec![get_kimi_code_sessions_dir()]
}

pub fn scan_sessions() -> Vec<SessionMeta> {
    let index_path = get_kimi_code_session_index_path();
    if index_path.exists() {
        let from_index = scan_from_index(&index_path);
        if !from_index.is_empty() {
            return from_index;
        }
    }
    // Fallback: walk session dirs directly
    scan_from_dirs()
}

fn scan_from_index(path: &Path) -> Vec<SessionMeta> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let reader = BufReader::new(file);
    let mut sessions = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<IndexEntry>(&line) else {
            continue;
        };
        let session_dir = PathBuf::from(&entry.session_dir);
        if !session_dir.exists() {
            continue;
        }
        let wire = session_dir.join("agents").join("main").join("wire.jsonl");
        let (created, updated, summary) = probe_wire(&wire);
        let title = summary.clone();
        sessions.push(SessionMeta {
            provider_id: PROVIDER_ID.to_string(),
            session_id: entry.session_id.clone(),
            title: summary,
            summary: title,
            project_dir: entry.work_dir,
            created_at: created,
            last_active_at: updated.or(created),
            source_path: Some(wire.display().to_string()),
            resume_command: Some(format!("kimi --resume {}", entry.session_id)),
        });
    }

    sessions
}

fn scan_from_dirs() -> Vec<SessionMeta> {
    let roots = session_roots();
    let mut sessions = Vec::new();
    for root in roots {
        if !root.exists() {
            continue;
        }
        let Ok(wd_entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for wd in wd_entries.flatten() {
            let wd_path = wd.path();
            if !wd_path.is_dir() {
                continue;
            }
            let Ok(session_entries) = std::fs::read_dir(&wd_path) else {
                continue;
            };
            for session in session_entries.flatten() {
                let session_path = session.path();
                if !session_path.is_dir() {
                    continue;
                }
                let session_id = session_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if session_id.is_empty() {
                    continue;
                }
                let wire = session_path.join("agents").join("main").join("wire.jsonl");
                if !wire.exists() {
                    continue;
                }
                let (created, updated, summary) = probe_wire(&wire);
                let title = summary.clone();
                sessions.push(SessionMeta {
                    provider_id: PROVIDER_ID.to_string(),
                    session_id: session_id.clone(),
                    title: summary,
                    summary: title,
                    project_dir: None,
                    created_at: created,
                    last_active_at: updated.or(created),
                    source_path: Some(wire.display().to_string()),
                    resume_command: Some(format!("kimi --resume {session_id}")),
                });
            }
        }
    }
    sessions
}

fn probe_wire(path: &Path) -> (Option<i64>, Option<i64>, Option<String>) {
    if !path.exists() {
        return (None, None, None);
    }
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (None, None, None),
    };
    let reader = BufReader::new(file);
    let mut created = None;
    let mut updated = None;
    let mut first_user: Option<String> = None;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let typ = value.get("type").and_then(Value::as_str).unwrap_or("");
        if typ == "metadata" {
            if let Some(ts) = value
                .get("created_at")
                .and_then(Value::as_i64)
                .or_else(|| value.get("created_at").and_then(Value::as_u64).map(|u| u as i64))
            {
                created = Some(normalize_ts(ts));
            }
        }
        if let Some(ts) = value
            .get("time")
            .and_then(Value::as_i64)
            .or_else(|| value.get("timestamp").and_then(Value::as_i64))
            .or_else(|| value.get("created_at").and_then(Value::as_i64))
        {
            updated = Some(normalize_ts(ts));
        }
        if first_user.is_none() && (typ == "user.message" || typ == "user") {
            let text = extract_message_text(&value);
            if !text.trim().is_empty() {
                first_user = Some(truncate_summary(&text, TITLE_MAX_CHARS).to_string());
            }
        }
    }

    (created, updated, first_user)
}

fn normalize_ts(ts: i64) -> i64 {
    // wire.jsonl uses epoch ms already in practice; if seconds, scale up
    if ts > 1_000_000_000_000 {
        ts
    } else if ts > 1_000_000_000 {
        ts * 1000
    } else {
        // Sub-second or invalid: keep as-is (caller may supply RFC3339 separately)
        ts
    }
}

fn extract_message_text(value: &Value) -> String {
    if let Some(content) = value.get("content") {
        let text = extract_text(content);
        if !text.is_empty() {
            return text;
        }
    }
    if let Some(text) = value.get("text").and_then(Value::as_str) {
        return text.to_string();
    }
    if let Some(message) = value.get("message") {
        return extract_text(message);
    }
    String::new()
}

/// Delete a Kimi Code session directory (parent of agents/main/wire.jsonl).
pub fn delete_session(
    _root: &Path,
    source_path: &Path,
    _session_id: &str,
) -> Result<bool, String> {
    // source_path points at wire.jsonl → session dir is ../../..
    let session_dir = source_path
        .parent() // main
        .and_then(|p| p.parent()) // agents
        .and_then(|p| p.parent()); // session_*
    let Some(session_dir) = session_dir else {
        return Err(format!(
            "Invalid Kimi Code session path: {}",
            source_path.display()
        ));
    };
    if !session_dir.exists() {
        return Ok(false);
    }
    std::fs::remove_dir_all(session_dir)
        .map_err(|e| format!("Failed to delete Kimi Code session {}: {e}", session_dir.display()))?;
    Ok(true)
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let file =
        File::open(path).map_err(|e| format!("Failed to open Kimi Code wire log: {e}"))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let typ = value.get("type").and_then(Value::as_str).unwrap_or("");
        let role = match typ {
            "user.message" | "user" => "user",
            "assistant.message" | "assistant" => "assistant",
            "system.message" | "system" | "config.update" => "system",
            "tool" | "tool.result" | "tool_result" => "tool",
            // Skip pure metadata / internal events
            "metadata" | "usage" | "token.usage" => continue,
            other if other.contains("user") => "user",
            other if other.contains("assistant") => "assistant",
            _ => continue,
        };
        let content = extract_message_text(&value);
        if content.trim().is_empty() && role != "tool" {
            continue;
        }
        let ts = value
            .get("time")
            .or_else(|| value.get("timestamp"))
            .or_else(|| value.get("created_at"))
            .and_then(|v| {
                parse_timestamp_to_ms(v).or_else(|| {
                    v.as_i64()
                        .or_else(|| v.as_u64().map(|u| u as i64))
                        .map(normalize_ts)
                })
            });

        messages.push(SessionMessage {
            role: role.to_string(),
            content,
            ts,
        });
    }

    Ok(messages)
}
