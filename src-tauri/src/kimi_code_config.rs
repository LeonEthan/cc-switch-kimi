//! Kimi Code CLI configuration (`~/.kimi-code/config.toml`).
//!
//! Kimi Code uses **additive** multi-provider management:
//! - All providers coexist under `[providers.<id>]`
//! - Models are declared under `[models."<provider>/<alias>"]`
//! - Active selection is `default_model = "provider/alias"`
//!
//! MCP lives in a sibling file: `~/.kimi-code/mcp.json`.
//! Skills live under `~/.kimi-code/skills/`.
//! Override the home directory with `KIMI_CODE_HOME` or CC Switch settings.

use crate::config::{get_home_dir, write_text_file};
use crate::error::AppError;
use crate::settings::get_kimi_code_override_dir;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// ============================================================================
// Paths
// ============================================================================

/// Resolve Kimi Code home directory.
///
/// Priority:
/// 1. CC Switch settings override (`kimiCodeConfigDir`)
/// 2. `KIMI_CODE_HOME` environment variable (non-empty after trim)
/// 3. Platform default `~/.kimi-code`
pub fn get_kimi_code_dir() -> PathBuf {
    if let Some(override_dir) = get_kimi_code_override_dir() {
        return override_dir;
    }

    if let Some(raw) = std::env::var_os("KIMI_CODE_HOME") {
        let value = raw.to_string_lossy();
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    get_home_dir().join(".kimi-code")
}

pub fn get_kimi_code_config_path() -> PathBuf {
    get_kimi_code_dir().join("config.toml")
}

pub fn get_kimi_code_mcp_path() -> PathBuf {
    get_kimi_code_dir().join("mcp.json")
}

pub fn get_kimi_code_skills_dir() -> PathBuf {
    get_kimi_code_dir().join("skills")
}

pub fn get_kimi_code_sessions_dir() -> PathBuf {
    get_kimi_code_dir().join("sessions")
}

pub fn get_kimi_code_session_index_path() -> PathBuf {
    get_kimi_code_dir().join("session_index.jsonl")
}

fn write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// ============================================================================
// Settings config (DB / UI JSON fragment)
// ============================================================================

/// Single model entry stored in CC Switch `settingsConfig.models[]`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KimiCodeModel {
    /// Alias key used in model table (without provider prefix).
    pub id: String,
    /// Model id sent to the API. Defaults to `id` when empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_efforts: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<String>,
}

/// Provider fragment stored as `settingsConfig` in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KimiCodeProviderConfig {
    /// Provider protocol type: `kimi`, `openai`, `anthropic`, …
    #[serde(default = "default_provider_type")]
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<KimiCodeModel>,
    /// Preferred model id (without provider prefix) used for `default_model`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model_id: Option<String>,
    /// Optional custom HTTP headers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_headers: Option<BTreeMap<String, String>>,
    /// Internal marker for managed/OAuth providers imported from live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _cc_source: Option<String>,
}

fn default_provider_type() -> String {
    "anthropic".to_string()
}

impl Default for KimiCodeProviderConfig {
    fn default() -> Self {
        Self {
            r#type: default_provider_type(),
            api_key: None,
            base_url: Some("https://api.kimi.com/coding/".to_string()),
            models: vec![KimiCodeModel {
                id: "k3".to_string(),
                model: Some("k3".to_string()),
                max_context_size: Some(1_048_576),
                max_input_size: None,
                max_output_size: None,
                display_name: Some("K3".to_string()),
                capabilities: Some(vec![
                    "thinking".into(),
                    "always_thinking".into(),
                    "image_in".into(),
                    "video_in".into(),
                    "tool_use".into(),
                ]),
                support_efforts: Some(vec!["low".into(), "high".into(), "max".into()]),
                default_effort: Some("high".to_string()),
            }],
            default_model_id: Some("k3".to_string()),
            custom_headers: None,
            _cc_source: None,
        }
    }
}

pub const PROVIDER_SOURCE_MANAGED: &str = "managed";
pub const PROVIDER_SOURCE_USER: &str = "user";

// ============================================================================
// TOML helpers
// ============================================================================

fn read_config_text() -> Result<String, AppError> {
    let path = get_kimi_code_config_path();
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))
}

fn parse_document(text: &str) -> Result<toml_edit::DocumentMut, AppError> {
    if text.trim().is_empty() {
        return Ok(toml_edit::DocumentMut::new());
    }
    text.parse::<toml_edit::DocumentMut>().map_err(|e| {
        AppError::localized(
            "provider.kimicode.config.invalid_toml",
            format!("Kimi Code config.toml 格式错误: {e}"),
            format!("Invalid Kimi Code config.toml: {e}"),
        )
    })
}

fn write_document(doc: &toml_edit::DocumentMut) -> Result<(), AppError> {
    let path = get_kimi_code_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }
    let content = doc.to_string();
    write_text_file(&path, &content)?;
    log::debug!("Kimi Code config written to {path:?}");
    Ok(())
}

fn ensure_table<'a>(
    item: &'a mut toml_edit::Item,
) -> Result<&'a mut toml_edit::Table, AppError> {
    if item.is_none() {
        *item = toml_edit::Item::Table(toml_edit::Table::new());
    }
    item.as_table_mut().ok_or_else(|| {
        AppError::localized(
            "provider.kimicode.config.not_table",
            "Kimi Code 配置节点必须是表结构",
            "Kimi Code configuration node must be a table",
        )
    })
}

fn set_string(table: &mut toml_edit::Table, key: &str, value: &str) {
    table.insert(key, toml_edit::value(value));
}

fn set_optional_string(table: &mut toml_edit::Table, key: &str, value: Option<&str>) {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => set_string(table, key, v),
        None => {
            table.remove(key);
        }
    }
}

fn set_u64(table: &mut toml_edit::Table, key: &str, value: u64) {
    table.insert(key, toml_edit::value(value as i64));
}

fn set_string_array(table: &mut toml_edit::Table, key: &str, values: &[String]) {
    let mut arr = toml_edit::Array::new();
    for v in values {
        arr.push(v.as_str());
    }
    table.insert(key, toml_edit::Item::Value(toml_edit::Value::Array(arr)));
}

fn table_string(table: &toml_edit::Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn table_u64(table: &toml_edit::Table, key: &str) -> Option<u64> {
    table.get(key).and_then(|item| {
        item.as_integer()
            .and_then(|n| u64::try_from(n).ok())
            .or_else(|| {
                item.as_str()
                    .and_then(|s| s.trim().parse::<u64>().ok())
            })
    })
}

fn table_string_array(table: &toml_edit::Table, key: &str) -> Option<Vec<String>> {
    table.get(key).and_then(|item| {
        item.as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
    })
}

// ============================================================================
// Provider CRUD
// ============================================================================

/// Whether a provider id is the OAuth-managed built-in account.
pub fn is_managed_provider_id(id: &str) -> bool {
    id == "managed:kimi-code" || id.starts_with("managed:")
}

/// Read all user-facing providers as a map of id → settings JSON.
///
/// Managed OAuth providers are included with `_cc_source = "managed"` so the UI
/// can treat them as read-only.
pub fn get_providers() -> Result<Map<String, Value>, AppError> {
    let text = read_config_text()?;
    let doc = parse_document(&text)?;
    let mut result = Map::new();

    let Some(providers) = doc.get("providers").and_then(|i| i.as_table()) else {
        return Ok(result);
    };

    let models_root = doc.get("models").and_then(|i| i.as_table());

    for (provider_id, item) in providers.iter() {
        let Some(provider_table) = item.as_table() else {
            continue;
        };

        let provider_type = table_string(provider_table, "type").unwrap_or_else(default_provider_type);
        let api_key = table_string(provider_table, "api_key");
        let base_url = table_string(provider_table, "base_url");

        let mut models = Vec::new();
        if let Some(models_table) = models_root {
            let prefix = format!("{provider_id}/");
            for (model_key, model_item) in models_table.iter() {
                if !model_key.starts_with(&prefix) {
                    continue;
                }
                let Some(model_table) = model_item.as_table() else {
                    continue;
                };
                // Prefer the suffix after provider/ as id
                let alias = model_key
                    .strip_prefix(&prefix)
                    .unwrap_or(model_key)
                    .to_string();
                // Skip if provider field mismatches
                if let Some(declared) = table_string(model_table, "provider") {
                    if declared != provider_id {
                        continue;
                    }
                }
                models.push(KimiCodeModel {
                    id: alias.clone(),
                    model: table_string(model_table, "model").or(Some(alias)),
                    max_context_size: table_u64(model_table, "max_context_size"),
                    max_input_size: table_u64(model_table, "max_input_size"),
                    max_output_size: table_u64(model_table, "max_output_size"),
                    display_name: table_string(model_table, "display_name"),
                    capabilities: table_string_array(model_table, "capabilities"),
                    support_efforts: table_string_array(model_table, "support_efforts"),
                    default_effort: table_string(model_table, "default_effort"),
                });
            }
        }

        let default_model_id = doc
            .get("default_model")
            .and_then(|i| i.as_str())
            .and_then(|dm| {
                let prefix = format!("{provider_id}/");
                dm.strip_prefix(&prefix).map(|s| s.to_string())
            })
            .or_else(|| models.first().map(|m| m.id.clone()));

        let source = if is_managed_provider_id(provider_id) {
            PROVIDER_SOURCE_MANAGED
        } else {
            PROVIDER_SOURCE_USER
        };

        let config = KimiCodeProviderConfig {
            r#type: provider_type,
            api_key,
            base_url,
            models,
            default_model_id,
            custom_headers: None,
            _cc_source: Some(source.to_string()),
        };

        match serde_json::to_value(config) {
            Ok(value) => {
                result.insert(provider_id.to_string(), value);
            }
            Err(e) => {
                log::warn!("Failed to serialize Kimi Code provider '{provider_id}': {e}");
            }
        }
    }

    Ok(result)
}

fn parse_provider_config(settings_config: Value) -> Result<KimiCodeProviderConfig, AppError> {
    serde_json::from_value(settings_config).map_err(|e| {
        AppError::localized(
            "provider.kimicode.config.invalid",
            format!("Kimi Code 供应商配置无效: {e}"),
            format!("Invalid Kimi Code provider config: {e}"),
        )
    })
}

fn ensure_writable_provider(id: &str) -> Result<(), AppError> {
    if is_managed_provider_id(id) {
        return Err(AppError::localized(
            "provider.kimicode.managed.readonly",
            format!("托管供应商 '{id}' 由 Kimi Code OAuth 管理，请在 CLI 中使用 /login 修改"),
            format!(
                "Managed provider '{id}' is controlled by Kimi Code OAuth; use /login in the CLI"
            ),
        ));
    }
    Ok(())
}

/// Mutate a config document: write `[providers.<id>]` + this provider's models.
/// Caller must hold [`write_lock`].
fn apply_provider_to_doc(
    doc: &mut toml_edit::DocumentMut,
    id: &str,
    config: &KimiCodeProviderConfig,
) -> Result<(), AppError> {
    {
        let providers_item = doc
            .entry("providers")
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        let providers = ensure_table(providers_item)?;
        let provider_item = providers
            .entry(id)
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        let provider_table = ensure_table(provider_item)?;

        set_string(provider_table, "type", config.r#type.trim());
        set_optional_string(provider_table, "api_key", config.api_key.as_deref());
        set_optional_string(provider_table, "base_url", config.base_url.as_deref());
        provider_table.remove("oauth");

        if let Some(headers) = &config.custom_headers {
            if !headers.is_empty() {
                let mut headers_table = toml_edit::Table::new();
                for (k, v) in headers {
                    set_string(&mut headers_table, k, v);
                }
                provider_table.insert("custom_headers", toml_edit::Item::Table(headers_table));
            } else {
                provider_table.remove("custom_headers");
            }
        }
    }

    {
        let models_item = doc
            .entry("models")
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        let models = ensure_table(models_item)?;

        let prefix = format!("{id}/");
        let keys_to_remove: Vec<String> = models
            .iter()
            .filter_map(|(k, _)| {
                if k.starts_with(&prefix) {
                    Some(k.to_string())
                } else {
                    None
                }
            })
            .collect();
        for key in keys_to_remove {
            models.remove(&key);
        }

        for model in &config.models {
            let alias = model.id.trim();
            if alias.is_empty() {
                continue;
            }
            let model_key = format!("{id}/{alias}");
            let model_item = models
                .entry(model_key.as_str())
                .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
            let model_table = ensure_table(model_item)?;

            set_string(model_table, "provider", id);
            let api_model = model
                .model
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(alias);
            set_string(model_table, "model", api_model);

            if let Some(n) = model.max_context_size {
                set_u64(model_table, "max_context_size", n);
            } else {
                set_u64(model_table, "max_context_size", 262_144);
            }
            if let Some(n) = model.max_input_size {
                set_u64(model_table, "max_input_size", n);
            }
            if let Some(n) = model.max_output_size {
                set_u64(model_table, "max_output_size", n);
            }
            set_optional_string(model_table, "display_name", model.display_name.as_deref());
            if let Some(caps) = &model.capabilities {
                if !caps.is_empty() {
                    set_string_array(model_table, "capabilities", caps);
                }
            }
            if let Some(efforts) = &model.support_efforts {
                if !efforts.is_empty() {
                    set_string_array(model_table, "support_efforts", efforts);
                }
            }
            set_optional_string(
                model_table,
                "default_effort",
                model.default_effort.as_deref(),
            );
        }
    }
    Ok(())
}

fn resolve_default_model_id(provider_id: &str, config: &KimiCodeProviderConfig) -> String {
    let model_id = config
        .default_model_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| config.models.first().map(|m| m.id.as_str()))
        .unwrap_or("k3");
    format!("{provider_id}/{model_id}")
}

/// Mutate document: set top-level `default_model`. Caller must hold lock.
fn apply_default_model_to_doc(
    doc: &mut toml_edit::DocumentMut,
    provider_id: &str,
    config: &KimiCodeProviderConfig,
) {
    doc.insert(
        "default_model",
        toml_edit::value(resolve_default_model_id(provider_id, config)),
    );
}

/// Upsert a provider into live `config.toml` (additive).
///
/// Writes `[providers.<id>]` and replaces this provider's `[models.*]` entries.
/// Does **not** change `default_model` — use [`upsert_and_select`] when switching.
pub fn set_provider(id: &str, settings_config: Value) -> Result<(), AppError> {
    ensure_writable_provider(id)?;
    let config = parse_provider_config(settings_config)?;

    let _guard = write_lock().lock().map_err(|e| {
        AppError::Message(format!("Failed to lock Kimi Code config for write: {e}"))
    })?;

    let text = read_config_text()?;
    let mut doc = parse_document(&text)?;
    apply_provider_to_doc(&mut doc, id, &config)?;
    write_document(&doc)
}

/// Atomically upsert provider + set `default_model` under one lock/write.
pub fn upsert_and_select(id: &str, settings_config: Value) -> Result<(), AppError> {
    ensure_writable_provider(id)?;
    let config = parse_provider_config(settings_config)?;

    let _guard = write_lock().lock().map_err(|e| {
        AppError::Message(format!("Failed to lock Kimi Code config for write: {e}"))
    })?;

    let text = read_config_text()?;
    let mut doc = parse_document(&text)?;
    apply_provider_to_doc(&mut doc, id, &config)?;
    apply_default_model_to_doc(&mut doc, id, &config);
    write_document(&doc)
}

/// Remove a user provider and its model entries. Does not touch managed providers.
pub fn remove_provider(id: &str) -> Result<(), AppError> {
    if is_managed_provider_id(id) {
        return Err(AppError::localized(
            "provider.kimicode.managed.readonly",
            format!("托管供应商 '{id}' 由 Kimi Code OAuth 管理，无法删除"),
            format!("Managed provider '{id}' is controlled by Kimi Code OAuth and cannot be removed"),
        ));
    }

    let _guard = write_lock().lock().map_err(|e| {
        AppError::Message(format!("Failed to lock Kimi Code config for write: {e}"))
    })?;

    let text = read_config_text()?;
    if text.trim().is_empty() {
        return Ok(());
    }
    let mut doc = parse_document(&text)?;

    if let Some(providers) = doc.get_mut("providers").and_then(|i| i.as_table_mut()) {
        providers.remove(id);
    }

    if let Some(models) = doc.get_mut("models").and_then(|i| i.as_table_mut()) {
        let prefix = format!("{id}/");
        let keys: Vec<String> = models
            .iter()
            .filter_map(|(k, _)| {
                if k.starts_with(&prefix) {
                    Some(k.to_string())
                } else {
                    None
                }
            })
            .collect();
        for key in keys {
            models.remove(&key);
        }
    }

    // If default_model pointed at this provider, clear or reassign
    if let Some(dm) = doc
        .get("default_model")
        .and_then(|i| i.as_str())
        .map(|s| s.to_string())
    {
        let prefix = format!("{id}/");
        if dm.starts_with(&prefix) {
            // Prefer first remaining model
            let next = doc
                .get("models")
                .and_then(|i| i.as_table())
                .and_then(|t| t.iter().next().map(|(k, _)| k.to_string()));
            match next {
                Some(key) => {
                    doc.insert("default_model", toml_edit::value(key));
                }
                None => {
                    doc.remove("default_model");
                }
            }
        }
    }

    write_document(&doc)
}

/// Update `default_model` only (provider rows must already exist).
/// Prefer [`upsert_and_select`] when both upsert and selection are needed.
#[allow(dead_code)] // kept for callers that only need to retarget selection
pub fn apply_switch_defaults(provider_id: &str, settings_config: &Value) -> Result<(), AppError> {
    let config: KimiCodeProviderConfig =
        serde_json::from_value(settings_config.clone()).unwrap_or_default();

    let _guard = write_lock().lock().map_err(|e| {
        AppError::Message(format!("Failed to lock Kimi Code config for write: {e}"))
    })?;

    let text = read_config_text()?;
    let mut doc = parse_document(&text)?;
    apply_default_model_to_doc(&mut doc, provider_id, &config);
    write_document(&doc)
}

/// Provider id owning the current `default_model` (prefix before `/`).
pub fn get_default_provider_id() -> Result<Option<String>, AppError> {
    Ok(get_default_model()?.and_then(|dm| {
        dm.split_once('/')
            .map(|(provider, _)| provider.to_string())
            .filter(|s| !s.is_empty())
    }))
}

/// Return the current `default_model` string if set.
pub fn get_default_model() -> Result<Option<String>, AppError> {
    let text = read_config_text()?;
    let doc = parse_document(&text)?;
    Ok(doc
        .get("default_model")
        .and_then(|i| i.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()))
}

// ============================================================================
// MCP (mcp.json)
// ============================================================================

/// Read MCP servers from `mcp.json` as a map of id → server spec.
pub fn get_mcp_servers() -> Result<Map<String, Value>, AppError> {
    let path = get_kimi_code_mcp_path();
    if !path.exists() {
        return Ok(Map::new());
    }
    let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    if content.trim().is_empty() {
        return Ok(Map::new());
    }
    let root: Value = serde_json::from_str(&content).map_err(|e| {
        AppError::Config(format!(
            "Failed to parse Kimi Code mcp.json ({}): {e}",
            path.display()
        ))
    })?;
    Ok(root
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default())
}

fn write_mcp_servers(servers: &Map<String, Value>) -> Result<(), AppError> {
    let path = get_kimi_code_mcp_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }
    let root = json!({ "mcpServers": servers });
    crate::config::write_json_file(&path, &root)?;
    Ok(())
}

pub fn set_mcp_server(id: &str, config: Value) -> Result<(), AppError> {
    let _guard = write_lock().lock().map_err(|e| {
        AppError::Message(format!("Failed to lock Kimi Code config for write: {e}"))
    })?;
    let mut servers = get_mcp_servers()?;
    servers.insert(id.to_string(), config);
    write_mcp_servers(&servers)
}

pub fn remove_mcp_server(id: &str) -> Result<(), AppError> {
    let _guard = write_lock().lock().map_err(|e| {
        AppError::Message(format!("Failed to lock Kimi Code config for write: {e}"))
    })?;
    let mut servers = get_mcp_servers()?;
    servers.remove(id);
    write_mcp_servers(&servers)
}

// ============================================================================
// Live settings snapshot (for "open config" / read_live_settings)
// ============================================================================

/// Return a JSON snapshot of live config useful for diagnostics.
pub fn read_live_settings() -> Result<Value, AppError> {
    let text = read_config_text()?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    // Parse via toml → Value for JSON serialization
    let value: toml::Value = text.parse().map_err(|e| {
        AppError::localized(
            "provider.kimicode.config.invalid_toml",
            format!("Kimi Code config.toml 格式错误: {e}"),
            format!("Invalid Kimi Code config.toml: {e}"),
        )
    })?;
    let json = toml_to_json(&value);
    Ok(json)
}

fn toml_to_json(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(s) => json!(s),
        toml::Value::Integer(i) => json!(i),
        toml::Value::Float(f) => json!(f),
        toml::Value::Boolean(b) => json!(b),
        toml::Value::Datetime(d) => json!(d.to_string()),
        toml::Value::Array(arr) => Value::Array(arr.iter().map(toml_to_json).collect()),
        toml::Value::Table(table) => {
            let mut map = Map::new();
            for (k, v) in table {
                map.insert(k.clone(), toml_to_json(v));
            }
            Value::Object(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home<F: FnOnce()>(f: F) {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("kimi-home");
        fs::create_dir_all(&home).unwrap();
        std::env::set_var("KIMI_CODE_HOME", &home);
        // Clear override by not setting settings — KIMI_CODE_HOME is used when override is None
        f();
        std::env::remove_var("KIMI_CODE_HOME");
    }

    #[test]
    fn set_and_get_provider_roundtrip() {
        with_temp_home(|| {
            let config = KimiCodeProviderConfig {
                r#type: "kimi".into(),
                api_key: Some("sk-test".into()),
                base_url: Some("https://api.kimi.com/coding/v1".into()),
                models: vec![KimiCodeModel {
                    id: "k3".into(),
                    model: Some("k3".into()),
                    max_context_size: Some(1_048_576),
                    display_name: Some("K3".into()),
                    ..Default::default()
                }],
                default_model_id: Some("k3".into()),
                ..Default::default()
            };
            let value = serde_json::to_value(&config).unwrap();
            upsert_and_select("acc-a", value).unwrap();

            let providers = get_providers().unwrap();
            let acc = providers.get("acc-a").unwrap();
            assert_eq!(acc["type"], "kimi");
            assert_eq!(acc["apiKey"], "sk-test");
            assert_eq!(get_default_model().unwrap().as_deref(), Some("acc-a/k3"));
            assert_eq!(
                get_default_provider_id().unwrap().as_deref(),
                Some("acc-a")
            );
        });
    }

    #[test]
    fn remove_provider_clears_default_model() {
        with_temp_home(|| {
            let config = KimiCodeProviderConfig::default();
            let value = serde_json::to_value(&config).unwrap();
            upsert_and_select("tmp", value).unwrap();
            remove_provider("tmp").unwrap();
            assert!(get_providers().unwrap().is_empty());
        });
    }
}
