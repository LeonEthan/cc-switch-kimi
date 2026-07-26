//! Pi (`pi` CLI) 会话用量追踪
//!
//! 从 `~/.pi/agent/sessions/--<cwd>--/<ts>_<uuid>.jsonl`（v3）中的 assistant
//! 消息 + compaction + branch_summary 事件提取 token 用量，写入
//! `proxy_request_logs`，覆盖无代理直连态下的统计。
//!
//! ## 数据流
//! ```text
//! session jsonl
//!   ├─ type=message, role=assistant, message.usage  → 带 model 字段的逐请求用量
//!   ├─ type=compaction,           entry.usage?     → Pi 自动摘要用量（无 model）
//!   └─ type=branch_summary,       entry.usage?     → 分支摘要用量（无 model）
//!        → 费用计算 → proxy_request_logs（data_source=pi_session）
//! ```
//!
//! ## 事件口径（实测 pi 0.82 / 0.83+ 会话文件）
//! - **assistant 消息**（`type == "message"` 且 `message.role == "assistant"`）
//!   字段：`message.model`、`message.usage.{input,output,cacheRead,cacheWrite}`、
//!   条目级 `timestamp`（RFC3339）、条目级 `id`（8 位十六进制，会话内唯一）。
//! - **compaction**（`type == "compaction"`，pi 自动上下文压缩时写）携带
//!   可选 `.usage`（同 Usage 结构），无 `.model`；我们用合成 `_pi_summary`
//!   标签写入（与 pi 自身的 `getUsageCostBreakdown` 把同种条目归到
//!   `Tools/summaries` 一桶的策略对齐）。
//! - **branch_summary**（`type == "branch_summary"`，分支切走时由 pi 摘要
//!   被放弃的分支）携带可选 `.usage`，同上用 `_pi_summary` 合成标签。
//! - `usage.input` 是新鲜输入（实测 input+output+cacheRead+cacheWrite == totalTokens），
//!   与 Anthropic 口径一致 → `input_token_semantics = FRESH`。
//! - 幂等键 `pi_session:{session_id}:{entry_id}`：条目 id 在会话内唯一且
//!   append-only，重扫/重启都稳定（compaction / branch_summary 同样适用）。
//! - 代理接管去重走统一指纹守卫（`should_skip_session_insert`）：/pi 路由
//!   产生的 proxy 行命中时跳过会话行，绝不双算。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::proxy::usage::calculator::CostCalculator;
use crate::proxy::usage::parser::TokenUsage;
use crate::services::session_usage::{
    get_sync_state, metadata_modified_nanos, update_sync_state, SessionSyncResult,
};
use crate::services::sql_helpers::INPUT_TOKEN_SEMANTICS_FRESH;
use crate::services::usage_stats::{find_model_pricing, should_skip_session_insert, DedupKey};
use rust_decimal::Decimal;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 单条用量事件（assistant 消息 / compaction / branch_summary）
#[derive(Debug, Clone, PartialEq, Eq)]
struct PiUsageEvent {
    /// 会话内唯一条目 id（参与 request_id）
    entry_id: String,
    /// epoch 秒（由条目时间戳换算）
    created_at: i64,
    /// 真实 model id 或合成标签（compaction/branch_summary → `_pi_summary`）
    model: String,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_tokens: u32,
    cache_creation_tokens: u32,
}

/// 同步 Pi 使用数据（从 session jsonl 会话日志）
pub fn sync_pi_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    let files = collect_session_files();

    let mut result = SessionSyncResult {
        files_scanned: files.len() as u32,
        ..Default::default()
    };

    for file_path in &files {
        match sync_single_session_file(db, file_path) {
            Ok(file_result) => result.merge(file_result),
            Err(e) => {
                let msg = format!("Pi 会话文件解析失败 {}: {e}", file_path.display());
                log::warn!("[PI-SYNC] {msg}");
                result.errors.push(msg);
            }
        }
    }

    if result.imported > 0 {
        log::info!(
            "[PI-SYNC] 同步完成: 导入 {} 条, 跳过 {} 条, 扫描 {} 个文件",
            result.imported,
            result.skipped,
            result.files_scanned
        );
    }

    Ok(result)
}

fn collect_session_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in crate::session_manager::providers::pi::session_roots() {
        collect_jsonl_files(&root, &mut files);
    }
    files
}

/// 固定两层布局：`<root>/--<cwd>--/*.jsonl`。非递归，避免符号链接环。
fn collect_jsonl_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(bucket) = fs::read_dir(&path) else {
            continue;
        };
        for file in bucket.flatten() {
            let file_path = file.path();
            if file_path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                files.push(file_path);
            }
        }
    }
}

fn sync_single_session_file(
    db: &Database,
    file_path: &Path,
) -> Result<SessionSyncResult, AppError> {
    let file_path_str = file_path.to_string_lossy().to_string();

    let metadata = fs::metadata(file_path)
        .map_err(|e| AppError::Config(format!("无法读取文件元数据: {e}")))?;
    let file_modified = metadata_modified_nanos(&metadata);

    let (last_modified, _last_offset) = get_sync_state(db, &file_path_str)?;
    if file_modified <= last_modified {
        return Ok(SessionSyncResult::default());
    }

    let content = fs::read_to_string(file_path)
        .map_err(|e| AppError::Config(format!("无法读取文件: {e}")))?;
    let (session_id, events) = parse_pi_usage_events(&content, file_path);

    let mut result = SessionSyncResult::default();

    for event in &events {
        if event.is_zero() {
            result.skipped += 1;
            continue;
        }

        let request_id = format!("pi_session:{session_id}:{}", event.entry_id);

        match insert_pi_session_entry(db, &request_id, event, &session_id) {
            Ok(true) => result.imported += 1,
            Ok(false) => result.skipped += 1,
            Err(e) => {
                log::warn!("[PI-SYNC] 插入失败 ({request_id}): {e}");
                result.skipped += 1;
            }
        }
    }

    update_sync_state(db, &file_path_str, file_modified, events.len() as i64)?;
    Ok(result)
}

impl PiUsageEvent {
    fn is_zero(&self) -> bool {
        self.input_tokens == 0
            && self.output_tokens == 0
            && self.cache_read_tokens == 0
            && self.cache_creation_tokens == 0
    }
}

/// 从 session jsonl 内容解析 assistant 消息 + compaction + branch_summary
/// 用量事件。
///
/// 返回 `(session_id, events)`：session_id 取自 `type == "session"` 头的 `id`，
/// 缺失时回落文件名 `<ts>_<uuid>.jsonl` 的 uuid 段。
fn parse_pi_usage_events(content: &str, file_path: &Path) -> (String, Vec<PiUsageEvent>) {
    let mut session_id: Option<String> = None;
    let mut events = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let typ = record.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if typ == "session" {
            session_id = record
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            continue;
        }

        // compaction / branch_summary：可选 .usage，没有 .model
        if typ == "compaction" || typ == "branch_summary" {
            if let Some(ev) = parse_summary_event(&record) {
                events.push(ev);
            }
            continue;
        }

        if typ != "message" {
            continue;
        }
        let Some(message) = record.get("message").filter(|m| m.is_object()) else {
            continue;
        };
        if message.get("role").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let Some(usage) = message.get("usage").filter(|u| u.is_object()) else {
            continue;
        };

        // 条目级 RFC3339 优先；message.timestamp（epoch ms）兜底
        let created_at = record
            .get("timestamp")
            .and_then(parse_ts_seconds)
            .or_else(|| message.get("timestamp").and_then(parse_ts_seconds))
            .unwrap_or(0);

        let model = message
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        events.push(PiUsageEvent {
            entry_id: record
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            created_at,
            model,
            input_tokens: json_u32(usage, "input"),
            output_tokens: json_u32(usage, "output"),
            cache_read_tokens: json_u32(usage, "cacheRead"),
            cache_creation_tokens: json_u32(usage, "cacheWrite"),
        });
    }

    // 丢弃 entry_id 为空的（理论上助理消息一定有 id；兜底）
    events.retain(|e| !e.entry_id.is_empty());

    let session_id = session_id
        .filter(|s| !s.is_empty())
        .or_else(|| {
            file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|stem| stem.rsplit('_').next())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    (session_id, events)
}

/// 解析 compaction / branch_summary 条目中的 LLM 生成摘要用量。
///
/// `entry.usage` 为可选且非空时才产出事件；entry id 缺失或非字符串时跳过。
/// 这两类条目都没有 `.model`，统一使用合成标签 `_pi_summary`——价格表
/// 查不到时 cost=0，但 token 计数仍然进入使用统计（与 pi 自身
/// `getUsageCostBreakdown` 把它们都归到 `Tools/summaries` 这一桶的策略对齐）。
fn parse_summary_event(record: &serde_json::Value) -> Option<PiUsageEvent> {
    let entry_id = record
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    let usage = record.get("usage").filter(|u| u.is_object())?;
    let created_at = record
        .get("timestamp")
        .and_then(parse_ts_seconds)
        .unwrap_or(0);
    Some(PiUsageEvent {
        entry_id: entry_id.to_string(),
        created_at,
        model: "_pi_summary".to_string(),
        input_tokens: json_u32(usage, "input"),
        output_tokens: json_u32(usage, "output"),
        cache_read_tokens: json_u32(usage, "cacheRead"),
        cache_creation_tokens: json_u32(usage, "cacheWrite"),
    })
}

fn json_u32(obj: &serde_json::Value, key: &str) -> u32 {
    obj.get(key)
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_i64().map(|i| i.max(0) as u64))
                .or_else(|| v.as_f64().map(|f| f.max(0.0) as u64))
        })
        .map(|n| n.min(u32::MAX as u64) as u32)
        .unwrap_or(0)
}

/// epoch 秒/毫秒数字或 RFC3339 字符串 → epoch 秒。
fn parse_ts_seconds(value: &serde_json::Value) -> Option<i64> {
    if let Some(n) = value.as_i64() {
        return Some(normalize_to_ms(n) / 1000);
    }
    if let Some(n) = value.as_u64() {
        return Some(normalize_to_ms(n.min(i64::MAX as u64) as i64) / 1000);
    }
    value
        .as_str()
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.timestamp())
}

fn normalize_to_ms(ts: i64) -> i64 {
    // > 1e12 视为已是毫秒；> 1e9 视为秒
    if ts > 1_000_000_000_000 {
        ts
    } else if ts > 1_000_000_000 {
        ts.saturating_mul(1000)
    } else {
        ts
    }
}

fn insert_pi_session_entry(
    db: &Database,
    request_id: &str,
    event: &PiUsageEvent,
    session_id: &str,
) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);

    let created_at = if event.created_at > 0 {
        event.created_at
    } else {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    };

    let dedup_key = DedupKey {
        app_type: "pi",
        model: &event.model,
        input_tokens: event.input_tokens,
        output_tokens: event.output_tokens,
        cache_read_tokens: event.cache_read_tokens,
        cache_creation_tokens: event.cache_creation_tokens,
        created_at,
    };
    if should_skip_session_insert(&conn, request_id, &dedup_key)? {
        return Ok(false);
    }

    let usage = TokenUsage {
        input_tokens: event.input_tokens,
        output_tokens: event.output_tokens,
        cache_read_tokens: event.cache_read_tokens,
        cache_creation_tokens: event.cache_creation_tokens,
        model: Some(event.model.clone()),
        message_id: None,
    };

    let (input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost) =
        match find_model_pricing(&conn, &event.model) {
            Some(pricing) => {
                let cost =
                    CostCalculator::calculate_for_app("pi", &usage, &pricing, Decimal::from(1));
                (
                    cost.input_cost.to_string(),
                    cost.output_cost.to_string(),
                    cost.cache_read_cost.to_string(),
                    cost.cache_creation_cost.to_string(),
                    cost.total_cost.to_string(),
                )
            }
            None => (
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
            ),
        };

    // INSERT OR IGNORE：request_id 幂等；input_token_semantics=FRESH（usage.input
    // 已是新鲜输入，实测 input+output+cacheRead+cacheWrite == totalTokens）。
    let inserted_rows = conn
        .execute(
            "INSERT OR IGNORE INTO proxy_request_logs (
            request_id, provider_id, app_type, model, request_model,
            input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
            input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
            latency_ms, first_token_ms, status_code, error_message, session_id,
            provider_type, is_streaming, cost_multiplier, created_at, data_source,
            input_token_semantics
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
            rusqlite::params![
                request_id,
                "_pi_session", // provider_id
                "pi",          // app_type
                event.model,
                event.model, // request_model = model
                event.input_tokens,
                event.output_tokens,
                event.cache_read_tokens,
                event.cache_creation_tokens,
                input_cost,
                output_cost,
                cache_read_cost,
                cache_creation_cost,
                total_cost,
                0i64,                   // latency_ms
                Option::<i64>::None,    // first_token_ms
                200i64,                 // status_code
                Option::<String>::None, // error_message
                session_id,
                Some("pi_session"), // provider_type
                1i64,                 // is_streaming
                "1.0",                // cost_multiplier
                created_at,
                "pi_session", // data_source
                INPUT_TOKEN_SEMANTICS_FRESH,
            ],
        )
        .map_err(|e| AppError::Database(format!("插入 Pi 会话日志失败: {e}")))?;

    Ok(inserted_rows > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use std::io::Write;
    use tempfile::tempdir;

    const HEADER: &str = r#"{"type":"session","version":3,"id":"sess-abc","timestamp":"2026-07-25T04:53:39.624Z","cwd":"/tmp/project"}"#;

    fn assistant_line(
        id: &str,
        ts: &str,
        model: &str,
        input: u32,
        output: u32,
        cache_read: u32,
        cache_write: u32,
    ) -> String {
        format!(
            r#"{{"type":"message","id":"{id}","parentId":null,"timestamp":"{ts}","message":{{"role":"assistant","content":[{{"type":"text","text":"ok"}}],"provider":"kimi-coding","model":"{model}","usage":{{"input":{input},"output":{output},"cacheRead":{cache_read},"cacheWrite":{cache_write},"totalTokens":{}}},"stopReason":"stop","timestamp":1784955336023}}}}"#,
            input + output + cache_read + cache_write
        )
    }

    #[test]
    fn parse_extracts_assistant_usage_and_skips_others() {
        let content = format!(
            "{}\n{}\n{}\n{}\n{}\n",
            HEADER,
            r#"{"type":"model_change","id":"568f71d3","timestamp":"2026-07-25T04:53:39.930Z","provider":"kimi-coding","modelId":"k3-256k"}"#,
            r#"{"type":"message","id":"694904fe","timestamp":"2026-07-25T04:55:27.834Z","message":{"role":"user","content":[{"type":"text","text":"hi"}],"timestamp":1784955327824}}"#,
            assistant_line(
                "666c87cd",
                "2026-07-25T04:55:36.023Z",
                "k3-256k",
                20420,
                124,
                512,
                0
            ),
            r#"{"type":"message","id":"t1","timestamp":"2026-07-25T04:55:40.000Z","message":{"role":"toolResult","toolName":"bash","content":[{"type":"text","text":"x"}],"timestamp":1784955340000}}"#,
        );
        let (session_id, events) = parse_pi_usage_events(&content, Path::new("/tmp/x.jsonl"));
        assert_eq!(session_id, "sess-abc");
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.entry_id, "666c87cd");
        assert_eq!(e.model, "k3-256k");
        assert_eq!(e.input_tokens, 20420);
        assert_eq!(e.output_tokens, 124);
        assert_eq!(e.cache_read_tokens, 512);
        assert_eq!(e.cache_creation_tokens, 0);
        assert!(e.created_at > 1_700_000_000);
    }

    /// ADR #6：Pi 自动上下文压缩 (`type=compaction`) 与分支摘要
    /// (`type=branch_summary`) 都可能携带 `.usage`（同 Usage 结构，没有 `.model`）。
    /// 这两种用量原本被静默丢弃，本测试钉死这两类事件被识别、用合成标签
    /// `_pi_summary`（与 pi 自身 `Tools/summaries` 桶键对齐）并按 entry_id
    /// 去重写入 `proxy_request_logs`。
    #[test]
    fn parse_extracts_compaction_and_branch_summary_usage() {
        let compaction = r#"{"type":"compaction","id":"cmp00001","parentId":"msg-prev","timestamp":"2026-07-25T05:00:00.000Z","summary":"...","firstKeptEntryId":"msg-keep","tokensBefore":180000,"usage":{"input":20420,"output":512,"cacheRead":1024,"cacheWrite":0,"totalTokens":21956,"cost":{"input":0.001,"output":0.0001,"cacheRead":0.0001,"cacheWrite":0,"total":0.0012}}}"#;
        let branch_summary = r#"{"type":"branch_summary","id":"bs000001","parentId":"branch-fork","timestamp":"2026-07-25T05:30:00.000Z","fromId":"msg-from","summary":"branch dropped","usage":{"input":4096,"output":256,"cacheRead":0,"cacheWrite":0,"totalTokens":4352,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}}}"#;
        // No usage → must be skipped, not crash.
        let compaction_no_usage =
            r#"{"type":"compaction","id":"cmp00002","timestamp":"2026-07-25T05:31:00.000Z"}"#;
        // Compaction with usage but no id → must be skipped (we need a stable
        // request_id key).
        let compaction_no_id = r#"{"type":"compaction","timestamp":"2026-07-25T05:32:00.000Z","usage":{"input":1,"output":2,"cacheRead":3,"cacheWrite":4,"totalTokens":10,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}}}"#;
        let content = format!(
            "{HEADER}\n{compaction}\n{branch_summary}\n{compaction_no_usage}\n{compaction_no_id}\n"
        );

        let (_session_id, events) = parse_pi_usage_events(&content, Path::new("/tmp/x.jsonl"));
        assert_eq!(
            events.len(),
            2,
            "two summary events with usage + id must be captured"
        );
        let a = events
            .iter()
            .find(|e| e.entry_id == "cmp00001")
            .expect("compaction event");
        assert_eq!(a.model, "_pi_summary");
        assert_eq!(a.input_tokens, 20420);
        assert_eq!(a.output_tokens, 512);
        assert_eq!(a.cache_read_tokens, 1024);
        assert_eq!(a.cache_creation_tokens, 0);
        assert!(a.created_at > 1_700_000_000, "RFC3339 timestamp must parse");

        let b = events
            .iter()
            .find(|e| e.entry_id == "bs000001")
            .expect("branch_summary event");
        assert_eq!(b.model, "_pi_summary");
        assert_eq!(b.input_tokens, 4096);
        assert_eq!(b.output_tokens, 256);
    }

    #[test]
    fn parse_falls_back_to_filename_uuid_and_message_ts() {
        let content = format!(
            "{}\n",
            r#"{"type":"message","id":"aa11bb22","message":{"role":"assistant","model":"m","usage":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0},"timestamp":1700000000000}}"#
        );
        let (session_id, events) = parse_pi_usage_events(
            &content,
            Path::new("/tmp/2026-07-25T04-53-39-624Z_019f979f-2ea8-7e64-a5d8-6248a0848568.jsonl"),
        );
        assert_eq!(session_id, "019f979f-2ea8-7e64-a5d8-6248a0848568");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].created_at, 1_700_000_000);
    }

    #[test]
    fn sync_imports_usage_idempotently_and_skips_zero() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let bucket = dir.path().join("--tmp-project--");
        fs::create_dir_all(&bucket).unwrap();
        let file = bucket.join("2026-07-25T04-53-39-624Z_019f979f.jsonl");
        {
            let mut f = fs::File::create(&file).unwrap();
            writeln!(f, "{HEADER}").unwrap();
            writeln!(
                f,
                "{}",
                assistant_line(
                    "666c87cd",
                    "2026-07-25T04:55:36.023Z",
                    "k3-256k",
                    100,
                    20,
                    10,
                    5
                )
            )
            .unwrap();
            // zero-token assistant row must be skipped
            writeln!(
                f,
                "{}",
                assistant_line(
                    "77777777",
                    "2026-07-25T04:55:37.023Z",
                    "k3-256k",
                    0,
                    0,
                    0,
                    0
                )
            )
            .unwrap();
        }

        let db = Database::memory()?;
        let first = sync_single_session_file(&db, &file)?;
        assert_eq!(first.imported, 1);
        assert_eq!(first.skipped, 1); // zero-token row

        // mtime unchanged → whole file skipped
        let second = sync_single_session_file(&db, &file)?;
        assert_eq!(second.imported, 0);

        // bump mtime via append → reparse; existing row deduped by request_id
        std::thread::sleep(std::time::Duration::from_millis(20));
        {
            let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
            writeln!(
                f,
                "{}",
                assistant_line(
                    "88888888",
                    "2026-07-25T04:55:38.023Z",
                    "k3-256k",
                    7,
                    3,
                    0,
                    0
                )
            )
            .unwrap();
        }

        let third = sync_single_session_file(&db, &file)?;
        assert_eq!(third.imported, 1, "only the new entry imports");
        assert!(third.skipped >= 2, "old entry + zero row are skipped");

        let conn = lock_conn!(db.conn);
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM proxy_request_logs WHERE data_source = 'pi_session'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 2);

        let row: (String, String, i64, i64) = conn.query_row(
            "SELECT request_id, app_type, input_tokens, input_token_semantics FROM proxy_request_logs WHERE data_source = 'pi_session' ORDER BY created_at LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        assert_eq!(row.0, "pi_session:sess-abc:666c87cd");
        assert_eq!(row.1, "pi");
        assert_eq!(row.2, 100);
        assert_eq!(row.3, INPUT_TOKEN_SEMANTICS_FRESH);

        Ok(())
    }

    #[test]
    fn insert_skips_when_matching_proxy_log_exists() -> Result<(), AppError> {
        let db = Database::memory()?;
        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model, request_model,
                    input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                    total_cost_usd, latency_ms, status_code, created_at, data_source
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    "proxy-pi-1",
                    "some-provider",
                    "pi",
                    "k3-256k",
                    "k3-256k",
                    100,
                    20,
                    10,
                    5,
                    "0.01",
                    100,
                    200,
                    1784955336i64,
                    "proxy"
                ],
            )?;
        }

        let event = PiUsageEvent {
            entry_id: "666c87cd".to_string(),
            created_at: 1784955336,
            model: "k3-256k".to_string(),
            input_tokens: 100,
            output_tokens: 20,
            cache_read_tokens: 10,
            cache_creation_tokens: 5,
        };
        let inserted =
            insert_pi_session_entry(&db, "pi_session:sess-abc:666c87cd", &event, "sess-abc")?;
        assert!(
            !inserted,
            "fingerprint-matched proxy row must suppress the session row"
        );

        let conn = lock_conn!(db.conn);
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM proxy_request_logs", [], |row| {
            row.get(0)
        })?;
        assert_eq!(count, 1);

        Ok(())
    }
}
