//! Kimi Code CLI 会话用量追踪
//!
//! 从 `~/.kimi-code/sessions/**/agents/main/wire.jsonl` 中的 `usage.record`
//! 事件提取 token 用量，写入 `proxy_request_logs`，覆盖无代理直连态下的统计。
//!
//! ## 数据流
//! ```text
//! wire.jsonl（usage.record, usageScope=turn）
//!   → 费用计算 → proxy_request_logs（data_source=kimicode_session）
//! ```
//!
//! ## 事件口径（实测 ~/.kimi-code wire.jsonl）
//! - 权威来源是顶层 `type == "usage.record"`，字段：
//!   `model`, `usage.{inputOther,output,inputCacheRead,inputCacheCreation}`,
//!   `usageScope`, `time`（epoch ms）。
//! - **只导入 `usageScope == "turn"`**：`session` 作用域是会话累计，再导入会双算。
//! - `inputOther` 是新鲜输入（Anthropic 风格），cache 读写单独计，**非**
//!   cache-inclusive。
//! - 与 `step.end` 嵌套 usage 近似 1:1；优先用 `usage.record`（字段更完整，
//!   含 model / usageScope）。
//! - 无本地 proxy 接管路径，不做代理行去重守卫（与 OpenCode 一致）。

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

/// 单条 turn 级 usage.record
#[derive(Debug, Clone, PartialEq, Eq)]
struct KimiUsageEvent {
    /// epoch 秒（由 wire 的 ms 时间戳换算）
    created_at: i64,
    /// 原始 ms 时间戳（参与 request_id，避免秒级碰撞）
    time_ms: i64,
    model: String,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_tokens: u32,
    cache_creation_tokens: u32,
    /// 文件内 turn-scope 序号（append-only 布局下跨重扫稳定）
    seq: u32,
}

/// 同步 Kimi Code 使用数据（从 wire.jsonl 会话日志）
pub fn sync_kimicode_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    let files = collect_wire_files();

    let mut result = SessionSyncResult {
        files_scanned: files.len() as u32,
        ..Default::default()
    };

    for file_path in &files {
        match sync_single_wire_file(db, file_path) {
            Ok(file_result) => result.merge(file_result),
            Err(e) => {
                let msg = format!("Kimi Code 会话文件解析失败 {}: {e}", file_path.display());
                log::warn!("[KIMICODE-SYNC] {msg}");
                result.errors.push(msg);
            }
        }
    }

    if result.imported > 0 {
        log::info!(
            "[KIMICODE-SYNC] 同步完成: 导入 {} 条, 跳过 {} 条, 扫描 {} 个文件",
            result.imported,
            result.skipped,
            result.files_scanned
        );
    }

    Ok(result)
}

fn collect_wire_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in crate::session_manager::providers::kimicode::session_roots() {
        collect_files_named(&root, "wire.jsonl", &mut files);
    }
    files
}

fn collect_files_named(root: &Path, name: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_named(&path, name, files);
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            files.push(path);
        }
    }
}

fn sync_single_wire_file(db: &Database, file_path: &Path) -> Result<SessionSyncResult, AppError> {
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
    let events = parse_kimi_usage_events(&content);

    // session_id：…/session_<uuid>/agents/main/wire.jsonl → session_<uuid>
    let session_id = file_path
        .parent() // main
        .and_then(|p| p.parent()) // agents
        .and_then(|p| p.parent()) // session_*
        .and_then(|dir| dir.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut result = SessionSyncResult::default();

    for event in &events {
        if event.is_zero() {
            result.skipped += 1;
            continue;
        }

        // 幂等键：session + 毫秒时间 + 文件内序号 + 模型 + token 指纹。
        // wire 为 append-only；seq 保证同毫秒同模型同 token 的两笔也不撞。
        let request_id = format!(
            "kimicode_session:{session_id}:{}:{}:{}:{}:{}:{}:{}",
            event.time_ms,
            event.seq,
            event.model,
            event.input_tokens,
            event.output_tokens,
            event.cache_read_tokens,
            event.cache_creation_tokens
        );

        match insert_kimi_session_entry(db, &request_id, event, &session_id) {
            Ok(true) => result.imported += 1,
            Ok(false) => result.skipped += 1,
            Err(e) => {
                log::warn!("[KIMICODE-SYNC] 插入失败 ({request_id}): {e}");
                result.skipped += 1;
            }
        }
    }

    update_sync_state(db, &file_path_str, file_modified, events.len() as i64)?;
    Ok(result)
}

impl KimiUsageEvent {
    fn is_zero(&self) -> bool {
        self.input_tokens == 0
            && self.output_tokens == 0
            && self.cache_read_tokens == 0
            && self.cache_creation_tokens == 0
    }
}

/// 从 wire.jsonl 内容解析 turn 级 usage.record
fn parse_kimi_usage_events(content: &str) -> Vec<KimiUsageEvent> {
    let mut events = Vec::new();
    let mut seq: u32 = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if record.get("type").and_then(|v| v.as_str()) != Some("usage.record") {
            continue;
        }
        // 只入账 turn；session 是累计会双算
        let scope = record
            .get("usageScope")
            .and_then(|v| v.as_str())
            .unwrap_or("turn");
        if scope != "turn" {
            continue;
        }

        let Some(usage) = record.get("usage").filter(|u| u.is_object()) else {
            continue;
        };

        let Some(time_ms) = parse_wire_time_ms(record.get("time")) else {
            continue;
        };
        let created_at = time_ms / 1000;

        let model = record
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let input_tokens = json_u32(usage, "inputOther");
        let output_tokens = json_u32(usage, "output");
        let cache_read_tokens = json_u32(usage, "inputCacheRead");
        let cache_creation_tokens = json_u32(usage, "inputCacheCreation");

        events.push(KimiUsageEvent {
            created_at,
            time_ms,
            model,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            seq,
        });
        seq = seq.saturating_add(1);
    }

    events
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

/// wire.jsonl 的 `time` 实测为 epoch 毫秒数字；字符串 RFC3339 作兜底。
fn parse_wire_time_ms(value: Option<&serde_json::Value>) -> Option<i64> {
    let value = value?;
    if let Some(n) = value.as_i64() {
        return Some(normalize_to_ms(n));
    }
    if let Some(n) = value.as_u64() {
        return Some(normalize_to_ms(n.min(i64::MAX as u64) as i64));
    }
    value
        .as_str()
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.timestamp_millis())
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

fn insert_kimi_session_entry(
    db: &Database,
    request_id: &str,
    event: &KimiUsageEvent,
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
        app_type: "kimicode",
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
                let cost = CostCalculator::calculate_for_app(
                    "kimicode",
                    &usage,
                    &pricing,
                    Decimal::from(1),
                );
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

    // INSERT OR IGNORE：request_id 幂等；input_token_semantics=FRESH（inputOther
    // 已是新鲜输入，与 Claude/OpenCode 一致）。
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
                "_kimicode_session", // provider_id
                "kimicode",          // app_type
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
                Some("kimicode_session"), // provider_type
                1i64,                     // is_streaming
                "1.0",                    // cost_multiplier
                created_at,
                "kimicode_session", // data_source
                INPUT_TOKEN_SEMANTICS_FRESH,
            ],
        )
        .map_err(|e| AppError::Database(format!("插入 Kimi Code 会话日志失败: {e}")))?;

    Ok(inserted_rows > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use std::io::Write;
    use tempfile::tempdir;

    fn sample_turn_line(time_ms: i64, model: &str, input: u32, output: u32, cache_read: u32) -> String {
        format!(
            r#"{{"type":"usage.record","model":"{model}","usage":{{"inputOther":{input},"output":{output},"inputCacheRead":{cache_read},"inputCacheCreation":0}},"usageScope":"turn","time":{time_ms}}}"#
        )
    }

    #[test]
    fn parse_turn_records_skips_session_scope() {
        let content = format!(
            "{}\n{}\n{}",
            sample_turn_line(1_700_000_000_000, "kimi-code/kimi-for-coding", 100, 20, 50),
            r#"{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":999,"output":9,"inputCacheRead":1,"inputCacheCreation":0},"usageScope":"session","time":1700000001000}"#,
            sample_turn_line(1_700_000_002_000, "kimi-code/k3", 10, 5, 0),
        );
        let events = parse_kimi_usage_events(&content);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].model, "kimi-code/kimi-for-coding");
        assert_eq!(events[0].input_tokens, 100);
        assert_eq!(events[0].cache_read_tokens, 50);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[0].created_at, 1_700_000_000);
        assert_eq!(events[1].model, "kimi-code/k3");
        assert_eq!(events[1].seq, 1);
    }

    #[test]
    fn parse_ignores_non_usage_lines() {
        let content = r#"
{"type":"metadata","created_at":1}
{"type":"context.append_loop_event","event":{"type":"step.end","usage":{"inputOther":1,"output":1,"inputCacheRead":0,"inputCacheCreation":0}},"time":1700000000000}
"#;
        assert!(parse_kimi_usage_events(content).is_empty());
    }

    #[test]
    fn sync_imports_turn_records_idempotently() -> Result<(), AppError> {
        let dir = tempdir().unwrap();
        let session_dir = dir
            .path()
            .join("wd_test")
            .join("session_abc-123")
            .join("agents")
            .join("main");
        fs::create_dir_all(&session_dir).unwrap();
        let wire = session_dir.join("wire.jsonl");
        {
            let mut f = fs::File::create(&wire).unwrap();
            writeln!(
                f,
                "{}",
                sample_turn_line(1_700_000_000_000, "kimi-code/kimi-for-coding", 11260, 33, 13824)
            )
            .unwrap();
            writeln!(
                f,
                "{}",
                sample_turn_line(1_700_000_001_000, "kimi-code/kimi-for-coding", 200, 10, 0)
            )
            .unwrap();
            // session scope must not import
            writeln!(
                f,
                r#"{{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{{"inputOther":99999,"output":1,"inputCacheRead":0,"inputCacheCreation":0}},"usageScope":"session","time":1700000002000}}"#
            )
            .unwrap();
        }

        let db = Database::memory()?;
        // Point session root at temp by syncing the single file path directly
        let first = sync_single_wire_file(&db, &wire)?;
        assert_eq!(first.imported, 2);
        assert_eq!(first.skipped, 0);

        // mtime unchanged → skip whole file
        let second = sync_single_wire_file(&db, &wire)?;
        assert_eq!(second.imported, 0);

        // force re-read by bumping mtime via rewrite (same content)
        {
            let mut f = fs::File::create(&wire).unwrap();
            writeln!(
                f,
                "{}",
                sample_turn_line(1_700_000_000_000, "kimi-code/kimi-for-coding", 11260, 33, 13824)
            )
            .unwrap();
            writeln!(
                f,
                "{}",
                sample_turn_line(1_700_000_001_000, "kimi-code/kimi-for-coding", 200, 10, 0)
            )
            .unwrap();
        }
        // Ensure mtime advances on filesystems with coarse resolution
        std::thread::sleep(std::time::Duration::from_millis(20));
        {
            let mut f = fs::OpenOptions::new().append(true).open(&wire).unwrap();
            writeln!(f, r#"{{"type":"metadata"}}"#).unwrap();
        }

        let third = sync_single_wire_file(&db, &wire)?;
        // Both turns already present → skipped via INSERT OR IGNORE / dedup
        assert_eq!(third.imported, 0);
        assert!(third.skipped >= 2);

        let conn = lock_conn!(db.conn);
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM proxy_request_logs WHERE data_source = 'kimicode_session'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 2);

        let app_type: String = conn.query_row(
            "SELECT app_type FROM proxy_request_logs WHERE data_source = 'kimicode_session' LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(app_type, "kimicode");

        let semantics: i64 = conn.query_row(
            "SELECT input_token_semantics FROM proxy_request_logs WHERE data_source = 'kimicode_session' LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(semantics, INPUT_TOKEN_SEMANTICS_FRESH);

        let input: i64 = conn.query_row(
            "SELECT input_tokens FROM proxy_request_logs WHERE input_tokens = 11260",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(input, 11260);

        Ok(())
    }
}
