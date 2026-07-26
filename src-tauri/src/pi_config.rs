//! Pi coding agent configuration (`~/.pi/agent`).
//!
//! Pi uses **additive** multi-provider management:
//! - All providers coexist under `providers.<id>` in `models.json` (JSONC).
//! - API keys live in `auth.json` as `{ "type": "api_key", "key": ... }` per
//!   provider id. OAuth entries (`{ "type": "oauth", ... }`) are owned and
//!   refreshed by Pi itself; CC Switch never reads them into its own storage,
//!   never overwrites them, and never deletes them (Pi-owned OAuth).
//! - Active selection is `defaultProvider` + `defaultModel` in `settings.json`.
//!
//! Non-destructive coexistence: edits to `models.json` are round-trip edits on
//! a JSONC AST so unknown keys, other providers, comments, and formatting are
//! preserved; `settings.json`/`auth.json` are mutated key-by-key on a
//! `serde_json::Value` round-trip. Writes validate the on-disk schema shape and
//! the installed Pi version first (compatibility gate).
//!
//! Skills live under `~/.pi/agent/skills/`, the global instructions file is
//! `~/.pi/agent/AGENTS.md`, sessions under `~/.pi/agent/sessions/`.
//! Override the agent directory with `PI_CODING_AGENT_DIR` or CC Switch
//! settings (`piConfigDir`).

use crate::config::{atomic_write, get_home_dir};
use crate::error::AppError;
use crate::settings::get_pi_override_dir;
use json_five::rt::parser::{
    from_str as rt_from_str, JSONKeyValuePair as RtJSONKeyValuePair,
    JSONObjectContext as RtJSONObjectContext, JSONText as RtJSONText, JSONValue as RtJSONValue,
    KeyValuePairContext as RtKeyValuePairContext,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// ============================================================================
// Compatibility gate
// ============================================================================

/// Minimum Pi version whose `models.json` merge semantics and session format
/// match what this module writes (merge-by-id stable since 0.52.7).
pub const MIN_PI_VERSION: &str = "0.52.7";

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts
        .next()
        .and_then(|p| {
            p.split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse::<u64>().ok())
        })
        .unwrap_or(0);
    Some((major, minor, patch))
}

/// Detect the installed Pi version via `pi --version` (cached for the process
/// lifetime). Returns `None` when the binary is not installed or not runnable;
/// an unknown version does not block writes (the config files remain valid for
/// a Pi installed later).
pub fn detect_pi_version() -> Option<String> {
    static VERSION: OnceLock<Option<String>> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            let output = std::process::Command::new("pi")
                .arg("--version")
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .clone()
}

/// Compatibility gate: refuse unsafe writes when the installed Pi version is
/// known to be older than [`MIN_PI_VERSION`].
fn assert_pi_compatible() -> Result<(), AppError> {
    if let Some(version) = detect_pi_version() {
        if let (Some(found), Some(min)) = (parse_version(&version), parse_version(MIN_PI_VERSION)) {
            if found < min {
                return Err(AppError::localized(
                    "provider.pi.incompatible_version",
                    format!(
                        "检测到 Pi 版本 {version} 低于最低支持版本 {MIN_PI_VERSION}，已阻止写入配置。请升级 Pi 后重试。"
                    ),
                    format!(
                        "Pi version {version} is below the minimum supported {MIN_PI_VERSION}; config writes are blocked. Please upgrade Pi."
                    ),
                ));
            }
        }
    }
    Ok(())
}

// ============================================================================
// Paths
// ============================================================================

/// Resolve the Pi agent directory.
///
/// Priority:
/// 1. CC Switch settings override (`piConfigDir`)
/// 2. `PI_CODING_AGENT_DIR` environment variable (non-empty after trim, `~` expanded)
/// 3. Platform default `~/.pi/agent`
pub fn get_pi_dir() -> PathBuf {
    if let Some(override_dir) = get_pi_override_dir() {
        return override_dir;
    }

    if let Some(raw) = std::env::var_os("PI_CODING_AGENT_DIR") {
        let value = raw.to_string_lossy();
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return expand_tilde(trimmed);
        }
    }

    get_home_dir().join(".pi").join("agent")
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return get_home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        return get_home_dir().join(rest);
    }
    PathBuf::from(path)
}

pub fn get_pi_settings_path() -> PathBuf {
    get_pi_dir().join("settings.json")
}

pub fn get_pi_models_path() -> PathBuf {
    get_pi_dir().join("models.json")
}

pub fn get_pi_auth_path() -> PathBuf {
    get_pi_dir().join("auth.json")
}

pub fn get_pi_skills_dir() -> PathBuf {
    get_pi_dir().join("skills")
}

/// Resolve the Pi sessions directory.
///
/// Priority: `settings.json` `sessionDir` → `PI_CODING_AGENT_SESSION_DIR`
/// → `<agent dir>/sessions`.
pub fn get_pi_sessions_dir() -> PathBuf {
    if let Ok(settings) = read_json_file(&get_pi_settings_path()) {
        if let Some(dir) = settings
            .get("sessionDir")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return expand_tilde(dir);
        }
    }
    if let Some(raw) = std::env::var_os("PI_CODING_AGENT_SESSION_DIR") {
        let value = raw.to_string_lossy();
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return expand_tilde(trimmed);
        }
    }
    get_pi_dir().join("sessions")
}

fn write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// ============================================================================
// Settings config (DB / UI JSON fragment)
// ============================================================================

pub const PROVIDER_SOURCE_MANAGED: &str = "managed";
pub const PROVIDER_SOURCE_USER: &str = "user";
pub const PROVIDER_SOURCE_OAUTH: &str = "oauth";

/// Whether a provider id is reserved/read-only for CC Switch management.
pub fn is_managed_provider_id(id: &str) -> bool {
    id.starts_with("managed:")
}

fn default_pi_api() -> String {
    "anthropic-messages".to_string()
}

/// Single model entry stored in CC Switch `settingsConfig.models[]`.
/// Mirrors Pi's `models.json` model definition (camelCase on the wire).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PiModel {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Per-model API override (defaults to the provider's `api`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<String>>,
    /// Cost object passthrough: `{ input, output, cacheRead, cacheWrite, tiers? }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Provider compatibility object passthrough (merged over provider compat).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compat: Option<Value>,
    /// Thinking-level map passthrough: level → string | null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level_map: Option<Map<String, Value>>,
}

/// Provider fragment stored as `settingsConfig` in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiProviderConfig {
    /// API protocol: `anthropic-messages`, `openai-completions`,
    /// `openai-responses`, `google-generative-ai`, …
    #[serde(default = "default_pi_api")]
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<PiModel>,
    /// Preferred model id used for `defaultModel` when switching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model_id: Option<String>,
    /// Human-readable provider name (models.json `name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Custom HTTP headers (models.json `headers`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    /// Provider-level compatibility object passthrough.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compat: Option<Value>,
    /// models.json `authHeader`: add `Authorization: Bearer <apiKey>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_header: Option<bool>,
    /// Internal marker for providers imported from live (`user` / `oauth` / `managed`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _cc_source: Option<String>,
}

impl Default for PiProviderConfig {
    fn default() -> Self {
        Self {
            r#type: default_pi_api(),
            api_key: None,
            base_url: None,
            models: Vec::new(),
            default_model_id: None,
            display_name: None,
            headers: None,
            compat: None,
            auth_header: None,
            _cc_source: None,
        }
    }
}

fn parse_provider_config(settings_config: Value) -> Result<PiProviderConfig, AppError> {
    serde_json::from_value(settings_config).map_err(|e| {
        AppError::localized(
            "provider.pi.config.invalid",
            format!("Pi 供应商配置无效: {e}"),
            format!("Invalid Pi provider config: {e}"),
        )
    })
}

fn ensure_writable_provider(id: &str) -> Result<(), AppError> {
    if is_managed_provider_id(id) {
        return Err(AppError::localized(
            "provider.pi.managed.readonly",
            format!("托管供应商 '{id}' 由 Pi 管理，请在 Pi 中修改"),
            format!("Managed provider '{id}' is controlled by Pi; edit it inside Pi"),
        ));
    }
    Ok(())
}

// ============================================================================
// Plain JSON file helpers (settings.json / auth.json — strict JSON owned by Pi)
// ============================================================================

fn read_json_file(path: &PathBuf) -> Result<Value, AppError> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content = fs::read_to_string(path).map_err(|e| AppError::io(path, e))?;
    if content.trim().is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_str(&content).map_err(|e| {
        AppError::localized(
            "provider.pi.config.invalid_json",
            format!("Pi 配置文件 {} 不是有效的 JSON: {e}", path.display()),
            format!("Pi config file {} is not valid JSON: {e}", path.display()),
        )
    })?;
    if !value.is_object() {
        return Err(AppError::localized(
            "provider.pi.config.not_object",
            format!("Pi 配置文件 {} 的顶层必须是 JSON 对象", path.display()),
            format!(
                "Pi config file {} must contain a JSON object",
                path.display()
            ),
        ));
    }
    Ok(value)
}

fn write_json_value(path: &PathBuf, value: &Value, mode_owner_only: bool) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }
    let content = serde_json::to_string_pretty(value)
        .map_err(|e| AppError::Message(format!("Failed to serialize Pi config: {e}")))?;
    atomic_write(path, format!("{content}\n").as_bytes())?;
    #[cfg(unix)]
    if mode_owner_only {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

// ============================================================================
// JSONC document helper for models.json (comment/format-preserving round-trip)
// ============================================================================

fn ensure_kvp_context(pair: &mut RtJSONKeyValuePair) -> &mut RtKeyValuePairContext {
    pair.context.get_or_insert_with(|| RtKeyValuePairContext {
        wsc: (String::new(), " ".to_string(), String::new(), None),
    })
}

fn extract_trailing_indent(separator_ws: &str) -> String {
    separator_ws
        .rsplit_once('\n')
        .map(|(_, tail)| tail.to_string())
        .unwrap_or_default()
}

fn derive_closing_ws_from_separator(separator_ws: &str) -> String {
    let Some((prefix, indent)) = separator_ws.rsplit_once('\n') else {
        return String::new();
    };

    let reduced_indent = if indent.ends_with('\t') {
        &indent[..indent.len().saturating_sub(1)]
    } else if indent.ends_with("  ") {
        &indent[..indent.len().saturating_sub(2)]
    } else if indent.ends_with(' ') {
        &indent[..indent.len().saturating_sub(1)]
    } else {
        indent
    };

    format!("{prefix}\n{reduced_indent}")
}

fn derive_entry_separator(leading_ws: &str) -> String {
    if leading_ws.is_empty() {
        return String::new();
    }
    if leading_ws.contains('\n') {
        return format!("\n{}", extract_trailing_indent(leading_ws));
    }
    String::new()
}

fn value_to_rt_value(value: &Value, parent_indent: &str) -> Result<RtJSONValue, AppError> {
    // Serialize with serde_json (valid JSON5), re-indent to match the target
    // nesting level, then re-parse into the round-trip AST for insertion.
    let source = serde_json::to_string_pretty(value)
        .map_err(|e| AppError::Message(format!("Failed to serialize Pi provider: {e}")))?;
    let adjusted = reindent_json_block(&source, parent_indent);
    let text = rt_from_str(&adjusted).map_err(|e| {
        AppError::Message(format!(
            "Failed to parse generated Pi provider JSON: {}",
            e.message
        ))
    })?;
    Ok(text.value)
}

fn reindent_json_block(source: &str, parent_indent: &str) -> String {
    if parent_indent.is_empty() || !source.contains('\n') {
        return source.to_string();
    }
    let mut lines = source.lines();
    let Some(first_line) = lines.next() else {
        return String::new();
    };
    let mut result = String::from(first_line);
    for line in lines {
        result.push('\n');
        result.push_str(parent_indent);
        result.push_str(line);
    }
    result
}

fn make_pair(key: &str, value: RtJSONValue, closing_ws: String) -> RtJSONKeyValuePair {
    RtJSONKeyValuePair {
        key: make_json5_key(key),
        value,
        context: Some(RtKeyValuePairContext {
            wsc: (String::new(), " ".to_string(), closing_ws, None),
        }),
    }
}

fn make_json5_key(key: &str) -> RtJSONValue {
    // Pi parses models.json as JSONC via `stripJsonComments` + strict
    // `JSON.parse` — comments survive, but bare identifier keys and trailing
    // commas do not. Always emit quoted keys on write. (Reading stays
    // tolerant: `json5_key_name` also matches `Identifier` keys.)
    RtJSONValue::DoubleQuotedString(key.to_string())
}

fn json5_key_name(key: &RtJSONValue) -> Option<&str> {
    match key {
        RtJSONValue::Identifier(name)
        | RtJSONValue::DoubleQuotedString(name)
        | RtJSONValue::SingleQuotedString(name) => Some(name),
        _ => None,
    }
}

/// Upsert `key` into a JSON object AST node, preserving sibling pairs,
/// comments, and formatting. `value` replaces any existing entry wholesale.
fn upsert_object_pair(obj: &mut RtJSONValue, key: &str, value: Value) -> Result<(), AppError> {
    let RtJSONValue::JSONObject {
        key_value_pairs,
        context,
    } = obj
    else {
        return Err(AppError::localized(
            "provider.pi.models.not_object",
            "Pi models.json 节点必须是 JSON 对象",
            "Pi models.json node must be a JSON object",
        ));
    };

    if key_value_pairs.is_empty() && context.as_ref().map(|c| c.wsc.0.is_empty()).unwrap_or(true) {
        *context = Some(RtJSONObjectContext {
            wsc: ("\n  ".to_string(),),
        });
    }

    let leading_ws = context
        .as_ref()
        .map(|c| c.wsc.0.clone())
        .unwrap_or_default();
    let entry_separator_ws = derive_entry_separator(&leading_ws);
    let child_indent = extract_trailing_indent(&leading_ws);
    let new_value = value_to_rt_value(&value, &child_indent)?;

    if let Some(existing) = key_value_pairs
        .iter_mut()
        .find(|pair| json5_key_name(&pair.key) == Some(key))
    {
        existing.value = new_value;
        return Ok(());
    }

    let new_pair = if let Some(last_pair) = key_value_pairs.last_mut() {
        let last_ctx = ensure_kvp_context(last_pair);
        let closing_ws = if let Some(after_comma) = last_ctx.wsc.3.clone() {
            last_ctx.wsc.3 = Some(entry_separator_ws.clone());
            after_comma
        } else {
            let closing_ws = std::mem::take(&mut last_ctx.wsc.2);
            last_ctx.wsc.3 = Some(entry_separator_ws.clone());
            closing_ws
        };
        make_pair(key, new_value, closing_ws)
    } else {
        make_pair(
            key,
            new_value,
            derive_closing_ws_from_separator(&leading_ws),
        )
    };
    key_value_pairs.push(new_pair);
    Ok(())
}

/// Remove `key` from a JSON object AST node, repairing comma/closing
/// whitespace so the result stays valid strict JSON (Pi parses models.json
/// with `JSON.parse` after stripping comments — trailing commas would break).
fn remove_object_pair(obj: &mut RtJSONValue, key: &str) {
    let RtJSONValue::JSONObject {
        key_value_pairs, ..
    } = obj
    else {
        return;
    };
    let Some(index) = key_value_pairs
        .iter()
        .position(|pair| json5_key_name(&pair.key) == Some(key))
    else {
        return;
    };
    let was_last = index == key_value_pairs.len() - 1;
    let removed = key_value_pairs.remove(index);
    if was_last {
        if let Some(new_last) = key_value_pairs.last_mut() {
            let ctx = ensure_kvp_context(new_last);
            ctx.wsc.3 = None; // drop the now-dangling comma
            if ctx.wsc.2.is_empty() {
                if let Some(removed_ctx) = removed.context {
                    ctx.wsc.2 = removed_ctx.wsc.2; // keep the closing-brace whitespace
                }
            }
        }
    }
}

/// A parsed `models.json` round-trip document. Only the `providers.<id>`
/// subtree is ever mutated; everything else (unknown keys, comments,
/// formatting) is preserved byte-for-byte.
struct PiModelsDocument {
    path: PathBuf,
    original_source: Option<String>,
    text: RtJSONText,
}

impl PiModelsDocument {
    fn load() -> Result<Self, AppError> {
        let path = get_pi_models_path();
        let original_source = if path.exists() {
            Some(fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?)
        } else {
            None
        };
        let source = original_source
            .clone()
            .unwrap_or_else(|| "{}\n".to_string());
        let text = rt_from_str(&source).map_err(|e| {
            AppError::localized(
                "provider.pi.models.invalid",
                format!("Pi models.json 解析失败: {}", e.message),
                format!("Failed to parse Pi models.json: {}", e.message),
            )
        })?;
        if !matches!(text.value, RtJSONValue::JSONObject { .. }) {
            return Err(AppError::localized(
                "provider.pi.config.not_object",
                "Pi models.json 的顶层必须是 JSON 对象",
                "Pi models.json must contain a JSON object",
            ));
        }
        Ok(Self {
            path,
            original_source,
            text,
        })
    }

    /// Upsert `providers.<id>`, creating the `providers` object when missing.
    fn set_provider(&mut self, id: &str, value: Value) -> Result<(), AppError> {
        let has_providers = matches!(
            &self.text.value,
            RtJSONValue::JSONObject { key_value_pairs, .. }
                if key_value_pairs.iter().any(|p| json5_key_name(&p.key) == Some("providers"))
        );
        if !has_providers {
            upsert_object_pair(&mut self.text.value, "providers", json!({}))?;
        }
        // Read the root whitespace AFTER the upsert: when the root was empty,
        // the upsert just created its context ("\n  "), and providers children
        // must inherit one level deeper than that — not the pre-upsert "".
        let root_ws = match &self.text.value {
            RtJSONValue::JSONObject { context, .. } => context
                .as_ref()
                .map(|c| c.wsc.0.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let RtJSONValue::JSONObject {
            key_value_pairs, ..
        } = &mut self.text.value
        else {
            unreachable!("root validated as object");
        };
        let providers_pair = key_value_pairs
            .iter_mut()
            .find(|pair| json5_key_name(&pair.key) == Some("providers"))
            .expect("providers pair just upserted");
        // A freshly created (empty) providers object inherits one extra indent
        // level from the root so nested entries line up with the file style.
        if root_ws.contains('\n') {
            if let RtJSONValue::JSONObject {
                key_value_pairs: inner,
                context,
            } = &mut providers_pair.value
            {
                if inner.is_empty() {
                    *context = Some(RtJSONObjectContext {
                        wsc: (format!("{}  ", root_ws),),
                    });
                }
            }
        }
        upsert_object_pair(&mut providers_pair.value, id, value)
    }

    /// Remove `providers.<id>` if present. No-op when absent.
    fn remove_provider(&mut self, id: &str) {
        let RtJSONValue::JSONObject {
            key_value_pairs, ..
        } = &mut self.text.value
        else {
            return;
        };
        let Some(providers_pair) = key_value_pairs
            .iter_mut()
            .find(|pair| json5_key_name(&pair.key) == Some("providers"))
        else {
            return;
        };
        remove_object_pair(&mut providers_pair.value, id);
    }

    /// Persist with an optimistic-concurrency check: if the file changed on
    /// disk since [`Self::load`], refuse to write (no clobbering concurrent
    /// edits).
    fn save(self) -> Result<(), AppError> {
        let current_source = if self.path.exists() {
            Some(fs::read_to_string(&self.path).map_err(|e| AppError::io(&self.path, e))?)
        } else {
            None
        };
        if current_source != self.original_source {
            return Err(AppError::localized(
                "provider.pi.models.changed_on_disk",
                "Pi models.json 在磁盘上已被修改，请重试",
                "Pi models.json changed on disk; please retry",
            ));
        }
        let next_source = self.text.to_string();
        if current_source.as_deref() == Some(next_source.as_str()) {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }
        atomic_write(&self.path, next_source.as_bytes())?;
        log::debug!("Pi models.json written to {:?}", self.path);
        Ok(())
    }
}

// ============================================================================
// auth.json (Pi-owned credential store; CC Switch only writes api_key entries)
// ============================================================================

fn read_auth_entry_type(auth: &Value, provider_id: &str) -> Option<String> {
    auth.get(provider_id)
        .and_then(|entry| entry.get("type"))
        .and_then(|t| t.as_str())
        .map(ToString::to_string)
}

/// Whether Pi owns an OAuth credential for this provider id.
pub fn provider_has_oauth_credential(provider_id: &str) -> bool {
    let auth = read_json_file(&get_pi_auth_path()).unwrap_or_else(|_| json!({}));
    read_auth_entry_type(&auth, provider_id).as_deref() == Some("oauth")
}

/// Mutate `auth.json` with an optimistic-concurrency retry: Pi may
/// concurrently refresh OAuth tokens into the same file, so the mutation is
/// re-applied on a fresh read when the file changed underneath us.
/// Caller must hold [`write_lock`].
fn mutate_auth_json<F>(mut mutate: F) -> Result<(), AppError>
where
    F: FnMut(&mut Map<String, Value>) -> Result<(), AppError>,
{
    let path = get_pi_auth_path();
    for attempt in 0..3 {
        let original_source = if path.exists() {
            Some(fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?)
        } else {
            None
        };
        let mut value = read_json_file(&path)?;
        let root = value.as_object_mut().ok_or_else(|| {
            AppError::localized(
                "provider.pi.config.not_object",
                "Pi auth.json 的顶层必须是 JSON 对象",
                "Pi auth.json must contain a JSON object",
            )
        })?;
        mutate(root)?;

        let current_source = if path.exists() {
            Some(fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?)
        } else {
            None
        };
        if current_source != original_source {
            if attempt < 2 {
                continue; // Pi refreshed a token concurrently; retry on fresh state
            }
            return Err(AppError::localized(
                "provider.pi.auth.changed_on_disk",
                "Pi auth.json 在磁盘上已被修改，请重试",
                "Pi auth.json changed on disk; please retry",
            ));
        }
        return write_json_value(&path, &value, true);
    }
    Ok(())
}

/// Upsert an API key for `provider_id`. Never overwrites a Pi-owned OAuth
/// credential: when one exists, the key write is skipped (OAuth wins).
/// Caller must hold [`write_lock`].
fn set_auth_api_key(provider_id: &str, api_key: Option<&str>) -> Result<(), AppError> {
    mutate_auth_json(|root| {
        let existing_type = root
            .get(provider_id)
            .and_then(|entry| entry.get("type"))
            .and_then(|t| t.as_str());
        if existing_type == Some("oauth") {
            // Pi-owned OAuth: leave the credential untouched.
            return Ok(());
        }
        match api_key.map(str::trim).filter(|k| !k.is_empty()) {
            Some(key) => {
                root.insert(
                    provider_id.to_string(),
                    json!({ "type": "api_key", "key": key }),
                );
            }
            None => {
                // Only remove api_key entries we own; never OAuth.
                if existing_type == Some("api_key") {
                    root.remove(provider_id);
                }
            }
        }
        Ok(())
    })
}

/// Remove an API key credential owned by CC Switch. OAuth entries are kept.
/// Caller must hold [`write_lock`].
fn remove_auth_api_key(provider_id: &str) -> Result<(), AppError> {
    mutate_auth_json(|root| {
        let existing_type = root
            .get(provider_id)
            .and_then(|entry| entry.get("type"))
            .and_then(|t| t.as_str());
        if existing_type == Some("api_key") {
            root.remove(provider_id);
        }
        Ok(())
    })
}

// ============================================================================
// models.json provider entry <-> PiProviderConfig
// ============================================================================

fn model_to_models_json(model: &PiModel) -> Value {
    let mut obj = Map::new();
    obj.insert("id".to_string(), json!(model.id.trim()));
    if let Some(name) = model
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        obj.insert("name".to_string(), json!(name));
    }
    if let Some(api) = model
        .api
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        obj.insert("api".to_string(), json!(api));
    }
    if let Some(reasoning) = model.reasoning {
        obj.insert("reasoning".to_string(), json!(reasoning));
    }
    if let Some(input) = &model.input {
        if !input.is_empty() {
            obj.insert("input".to_string(), json!(input));
        }
    }
    if let Some(cost) = &model.cost {
        if cost.is_object() {
            obj.insert("cost".to_string(), cost.clone());
        }
    }
    if let Some(n) = model.context_window {
        obj.insert("contextWindow".to_string(), json!(n));
    }
    if let Some(n) = model.max_tokens {
        obj.insert("maxTokens".to_string(), json!(n));
    }
    if let Some(compat) = &model.compat {
        if compat.is_object() {
            obj.insert("compat".to_string(), compat.clone());
        }
    }
    if let Some(map) = &model.thinking_level_map {
        obj.insert("thinkingLevelMap".to_string(), Value::Object(map.clone()));
    }
    Value::Object(obj)
}

fn model_from_models_json(value: &Value) -> Option<PiModel> {
    let obj = value.as_object()?;
    let id = obj.get("id").and_then(|v| v.as_str())?.trim().to_string();
    if id.is_empty() {
        return None;
    }
    Some(PiModel {
        id,
        name: obj
            .get("name")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        api: obj
            .get("api")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        reasoning: obj.get("reasoning").and_then(|v| v.as_bool()),
        input: obj.get("input").and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(ToString::to_string))
                    .collect()
            })
        }),
        cost: obj.get("cost").cloned(),
        context_window: obj.get("contextWindow").and_then(|v| v.as_u64()),
        max_tokens: obj.get("maxTokens").and_then(|v| v.as_u64()),
        compat: obj.get("compat").cloned(),
        thinking_level_map: obj
            .get("thinkingLevelMap")
            .and_then(|v| v.as_object())
            .cloned(),
    })
}

/// Build the `models.json` provider entry, preserving keys CC Switch does not
/// own (`oauth`, `modelOverrides`, hand-written `apiKey`, unknown extension
/// keys) from an existing entry.
fn provider_to_models_json(config: &PiProviderConfig, existing: Option<&Value>) -> Value {
    let mut obj = existing
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    let api = config.r#type.trim();
    if api.is_empty() {
        obj.remove("api");
    } else {
        obj.insert("api".to_string(), json!(api));
    }

    match config
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(url) => {
            obj.insert("baseUrl".to_string(), json!(url));
        }
        None => {
            obj.remove("baseUrl");
        }
    }

    match config
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(name) => {
            obj.insert("name".to_string(), json!(name));
        }
        None => {
            obj.remove("name");
        }
    }

    match &config.headers {
        Some(headers) if !headers.is_empty() => {
            obj.insert("headers".to_string(), json!(headers));
        }
        _ => {
            obj.remove("headers");
        }
    }

    match &config.compat {
        Some(compat) if compat.is_object() => {
            obj.insert("compat".to_string(), compat.clone());
        }
        _ => {
            obj.remove("compat");
        }
    }

    match config.auth_header {
        Some(v) => {
            obj.insert("authHeader".to_string(), json!(v));
        }
        None => {
            obj.remove("authHeader");
        }
    }

    let models: Vec<Value> = config
        .models
        .iter()
        .filter(|m| !m.id.trim().is_empty())
        .map(model_to_models_json)
        .collect();
    if models.is_empty() {
        obj.remove("models");
    } else {
        obj.insert("models".to_string(), Value::Array(models));
    }

    Value::Object(obj)
}

fn provider_from_models_json(
    entry: &Value,
    api_key: Option<String>,
    source: &str,
    default_model_id: Option<String>,
) -> PiProviderConfig {
    let obj = entry.as_object();
    let get_str = |key: &str| {
        obj.and_then(|o| o.get(key))
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
    };
    let models = obj
        .and_then(|o| o.get("models"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(model_from_models_json).collect())
        .unwrap_or_default();
    let headers = obj
        .and_then(|o| o.get("headers"))
        .and_then(|v| v.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<BTreeMap<_, _>>()
        });

    PiProviderConfig {
        r#type: get_str("api").unwrap_or_else(default_pi_api),
        api_key,
        base_url: get_str("baseUrl"),
        models,
        default_model_id,
        display_name: get_str("name"),
        headers,
        compat: obj.and_then(|o| o.get("compat")).cloned(),
        auth_header: obj
            .and_then(|o| o.get("authHeader"))
            .and_then(|v| v.as_bool()),
        _cc_source: Some(source.to_string()),
    }
}

// ============================================================================
// Provider CRUD (additive)
// ============================================================================

/// Read all providers from live `models.json` as a map of id → settings JSON.
///
/// API keys are attached from `auth.json` (`api_key` entries only). Providers
/// whose credential is Pi-owned OAuth are marked `_cc_source = "oauth"` and
/// returned without any credential material.
pub fn get_providers() -> Result<Map<String, Value>, AppError> {
    let models_value = read_models_json_value()?;
    let auth = read_json_file(&get_pi_auth_path()).unwrap_or_else(|_| json!({}));
    let default_provider = get_default_provider_id().unwrap_or(None);
    let default_model = get_default_model().unwrap_or(None);

    let mut result = Map::new();
    let Some(providers) = models_value.get("providers").and_then(|v| v.as_object()) else {
        return Ok(result);
    };

    for (provider_id, entry) in providers {
        if !entry.is_object() {
            continue;
        }
        let (api_key, source) = match read_auth_entry_type(&auth, provider_id).as_deref() {
            Some("oauth") => (None, PROVIDER_SOURCE_OAUTH),
            Some("api_key") => (
                auth.get(provider_id)
                    .and_then(|e| e.get("key"))
                    .and_then(|k| k.as_str())
                    .map(ToString::to_string),
                PROVIDER_SOURCE_USER,
            ),
            _ => (None, PROVIDER_SOURCE_USER),
        };
        let source = if is_managed_provider_id(provider_id) {
            PROVIDER_SOURCE_MANAGED
        } else {
            source
        };
        let default_model_id = if default_provider.as_deref() == Some(provider_id.as_str()) {
            default_model.clone()
        } else {
            None
        };

        let config = provider_from_models_json(entry, api_key, source, default_model_id);
        match serde_json::to_value(config) {
            Ok(value) => {
                result.insert(provider_id.clone(), value);
            }
            Err(e) => {
                log::warn!("Failed to serialize Pi provider '{provider_id}': {e}");
            }
        }
    }

    Ok(result)
}

/// Read `models.json` tolerantly (JSONC: comments and trailing commas allowed
/// on read; writes never introduce them).
fn read_models_json_value() -> Result<Value, AppError> {
    let path = get_pi_models_path();
    if !path.exists() {
        return Ok(json!({}));
    }
    let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    if content.trim().is_empty() {
        return Ok(json!({}));
    }
    let value: Value = json5::from_str(&content).map_err(|e| {
        AppError::localized(
            "provider.pi.models.invalid",
            format!("Pi models.json 解析失败: {e}"),
            format!("Failed to parse Pi models.json: {e}"),
        )
    })?;
    if !value.is_object() {
        return Err(AppError::localized(
            "provider.pi.config.not_object",
            "Pi models.json 的顶层必须是 JSON 对象",
            "Pi models.json must contain a JSON object",
        ));
    }
    Ok(value)
}

/// Shared upsert path: write `models.json` `providers.<id>` + `auth.json`
/// API key. Caller must hold [`write_lock`].
fn upsert_provider_locked(id: &str, config: &PiProviderConfig) -> Result<(), AppError> {
    let existing = {
        let current = read_models_json_value()?;
        current.get("providers").and_then(|p| p.get(id)).cloned()
    };
    let entry = provider_to_models_json(config, existing.as_ref());

    let mut doc = PiModelsDocument::load()?;
    doc.set_provider(id, entry)?;
    doc.save()?;

    set_auth_api_key(id, config.api_key.as_deref())
}

/// Upsert a provider into live config (additive). Does **not** change the
/// default selection — use [`upsert_and_select`] when switching.
pub fn set_provider(id: &str, settings_config: Value) -> Result<(), AppError> {
    assert_pi_compatible()?;
    ensure_writable_provider(id)?;
    let config = parse_provider_config(settings_config)?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Pi config for write: {e}")))?;

    upsert_provider_locked(id, &config)
}

/// Upsert provider + select it as Pi's default (`defaultProvider` /
/// `defaultModel` in `settings.json`).
pub fn upsert_and_select(id: &str, settings_config: Value) -> Result<(), AppError> {
    assert_pi_compatible()?;
    ensure_writable_provider(id)?;
    let config = parse_provider_config(settings_config)?;
    let default_model = resolve_default_model_id(&config);

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Pi config for write: {e}")))?;

    upsert_provider_locked(id, &config)?;

    write_default_selection(id, default_model.as_deref())
}

fn resolve_default_model_id(config: &PiProviderConfig) -> Option<String> {
    config
        .default_model_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            config
                .models
                .first()
                .map(|m| m.id.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

/// Write `defaultProvider` / `defaultModel` into `settings.json`, preserving
/// every other setting. Caller must hold [`write_lock`].
fn write_default_selection(provider_id: &str, model_id: Option<&str>) -> Result<(), AppError> {
    let path = get_pi_settings_path();
    let mut settings = read_json_file(&path)?;
    let root = settings.as_object_mut().ok_or_else(|| {
        AppError::localized(
            "provider.pi.config.not_object",
            "Pi settings.json 的顶层必须是 JSON 对象",
            "Pi settings.json must contain a JSON object",
        )
    })?;
    root.insert("defaultProvider".to_string(), json!(provider_id));
    match model_id {
        Some(model) => {
            root.insert("defaultModel".to_string(), json!(model));
        }
        None => {
            root.remove("defaultModel");
        }
    }
    write_json_value(&path, &settings, false)
}

/// Remove a provider from live config: drops the `models.json` entry and the
/// CC Switch-owned API key (OAuth credentials are never touched). Clears the
/// default selection when it pointed at the removed provider.
pub fn remove_provider(id: &str) -> Result<(), AppError> {
    assert_pi_compatible()?;
    ensure_writable_provider(id)?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Pi config for write: {e}")))?;

    let mut doc = PiModelsDocument::load()?;
    doc.remove_provider(id);
    doc.save()?;

    remove_auth_api_key(id)?;

    // Clear the selection when it referenced the removed provider.
    let path = get_pi_settings_path();
    let mut settings = read_json_file(&path)?;
    let pointed_at_removed = settings
        .get("defaultProvider")
        .and_then(|v| v.as_str())
        .map(|s| s == id)
        .unwrap_or(false);
    if pointed_at_removed {
        if let Some(root) = settings.as_object_mut() {
            root.remove("defaultProvider");
            root.remove("defaultModel");
        }
        write_json_value(&path, &settings, false)?;
    }
    Ok(())
}

/// Provider id owning Pi's current default selection (`settings.json`
/// `defaultProvider`).
pub fn get_default_provider_id() -> Result<Option<String>, AppError> {
    let settings = read_json_file(&get_pi_settings_path())?;
    Ok(settings
        .get("defaultProvider")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()))
}

/// Current `defaultModel` from `settings.json`, if set.
pub fn get_default_model() -> Result<Option<String>, AppError> {
    let settings = read_json_file(&get_pi_settings_path())?;
    Ok(settings
        .get("defaultModel")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()))
}

// ============================================================================
// Live settings snapshot (for "open config" / read_live_settings)
// ============================================================================

/// Return a JSON snapshot of Pi's live config for diagnostics.
///
/// OAuth credentials are redacted — Pi owns those tokens; they never leave the
/// process through this snapshot.
pub fn read_live_settings() -> Result<Value, AppError> {
    let settings = read_json_file(&get_pi_settings_path())?;
    let models = read_models_json_value()?;
    let mut auth = read_json_file(&get_pi_auth_path())?;
    if let Some(root) = auth.as_object_mut() {
        for (id, entry) in root.iter_mut() {
            let is_oauth = entry
                .get("type")
                .and_then(|t| t.as_str())
                .map(|t| t == "oauth")
                .unwrap_or(false);
            if is_oauth {
                *entry = json!({ "type": "oauth", "redacted": true });
                log::debug!("Redacted Pi OAuth credential for provider '{id}'");
            }
        }
    }
    Ok(json!({
        "settings": settings,
        "models": models,
        "auth": auth,
    }))
}

// ============================================================================
// Proxy takeover helpers (CC Switch local proxy)
// ============================================================================
//
// During proxy takeover CC Switch rewrites ONLY the selected provider's entry
// in models.json:
// - `baseUrl` -> `{proxy origin}/pi`. One URL serves both client protocols:
//   anthropic-messages clients append `/v1/messages`, openai-completions
//   clients append `/chat/completions`; the proxy exposes both under `/pi`.
// - `apiKey` -> an inline placeholder marker (detection signal, consistent with
//   other apps' takeover markers).
//
// auth.json is NEVER touched: OAuth entries stay Pi-owned (hard constraint),
// and api_key entries keep the real key. This is safe because Pi's auth
// resolution prefers the stored auth.json credential over models.json apiKey
// (pi-ai resolveProviderAuth: stored credential first, models.json key only as
// fallback), and the proxy strips client credentials and injects the
// DB-stored key either way. The inline placeholder is a *marker*, not a
// credential mask.

/// Live-config snapshot used for takeover backup/restore.
///
/// `modelsSource` is the raw models.json text (verbatim, so restores keep
/// comments/formatting); `settings` is settings.json; `authApiKeys` holds
/// auth.json `api_key` entries only — OAuth credentials are Pi-owned and never
/// copied into CC Switch persistence.
pub fn read_live_snapshot() -> Result<Value, AppError> {
    let models_path = get_pi_models_path();
    let models_source = if models_path.exists() {
        Some(fs::read_to_string(&models_path).map_err(|e| AppError::io(&models_path, e))?)
    } else {
        None
    };
    let settings = read_json_file(&get_pi_settings_path())?;
    let auth = read_json_file(&get_pi_auth_path()).unwrap_or_else(|_| json!({}));
    let mut auth_api_keys = Map::new();
    if let Some(root) = auth.as_object() {
        for (id, entry) in root {
            if entry.get("type").and_then(|t| t.as_str()) == Some("api_key") {
                if let Some(key) = entry.get("key").and_then(|k| k.as_str()) {
                    auth_api_keys.insert(id.clone(), json!(key));
                }
            }
        }
    }
    Ok(json!({
        "modelsSource": models_source,
        "settings": settings,
        "authApiKeys": Value::Object(auth_api_keys),
    }))
}

/// Restore a snapshot verbatim (backup restore path). Atomic writes.
///
/// When the snapshot has no `modelsSource` (models.json did not exist at
/// backup time) any takeover markers in a since-created models.json are
/// stripped instead of deleting a file the user may have edited.
pub fn write_live_snapshot(snapshot: &Value, placeholder: &str) -> Result<(), AppError> {
    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Pi config for write: {e}")))?;

    let models_path = get_pi_models_path();
    match snapshot.get("modelsSource").and_then(|v| v.as_str()) {
        Some(source) => {
            if let Some(parent) = models_path.parent() {
                fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
            }
            atomic_write(&models_path, source.as_bytes())?;
        }
        None => {
            if models_path.exists() {
                let _ = remove_takeover_markers_all_locked(placeholder)?;
            }
        }
    }
    if let Some(settings) = snapshot.get("settings") {
        write_json_value(&get_pi_settings_path(), settings, false)?;
    }
    Ok(())
}

/// Parse a models.json document tolerantly (JSONC).
pub(crate) fn parse_models_source(source: &str) -> Option<Value> {
    json5::from_str::<Value>(source)
        .ok()
        .filter(|v| v.is_object())
}

/// Whether a provider entry carries the proxy-takeover apiKey placeholder.
///
/// IMPORTANT: only the inline `apiKey == placeholder` string is the marker.
/// `baseUrl` alone is NEVER considered a marker: a user's own local provider
/// (e.g. `http://localhost:11434/v1` for Ollama, or any custom loopback they
/// typed into a CC Switch provider) shares the host shape with the proxy and
/// would otherwise be falsely classified as "taken over" — which used to
/// cause backups to be skipped, restore to refuse legitimate configs, and
/// `remove_takeover_markers_all` last-resort cleanup to silently strip the
/// user's `baseUrl`, leaving their `apiKey` pointed at the unintended default
/// upstream. The placeholder string is the only unambiguous signal: it can
/// only be written by `apply_takeover_and_select`, never by the user.
fn entry_has_takeover_markers(entry: &Value, placeholder: &str) -> bool {
    entry
        .as_object()
        .and_then(|obj| obj.get("apiKey"))
        .and_then(|v| v.as_str())
        == Some(placeholder)
}

/// Loopback URL check used ONLY to decide which `baseUrl` to strip from an
/// entry that already passed the marker check (apiKey == placeholder). This
/// is intentionally permissive (any loopback host) because reaching this
/// function implies the entry is one we wrote via `apply_takeover_and_select`,
/// whose baseUrl always pointed at the local proxy. Never used to classify
/// an entry as taken over (see `entry_has_takeover_markers`).
fn is_loopback_url(url: &str) -> bool {
    let url = url.trim();
    if !url.starts_with("http://") {
        return false;
    }
    let rest = &url["http://".len()..];
    rest.starts_with("127.0.0.1")
        || rest.starts_with("localhost")
        || rest.starts_with("0.0.0.0")
        || rest.starts_with("[::1]")
        || rest.starts_with("[::]")
        || rest.starts_with("::1")
        || rest.starts_with("::")
}

fn models_value_has_takeover_markers(models: &Value, placeholder: &str) -> bool {
    models
        .get("providers")
        .and_then(|v| v.as_object())
        .map(|providers| {
            providers
                .values()
                .any(|entry| entry_has_takeover_markers(entry, placeholder))
        })
        .unwrap_or(false)
}

/// Whether a value carries proxy-takeover markers. Accepts either a live
/// snapshot (with `modelsSource`) or a bare provider fragment (DB
/// `settings_config` with top-level `apiKey`/`baseUrl`).
pub fn has_takeover_markers(value: &Value, placeholder: &str) -> bool {
    if let Some(source) = value.get("modelsSource") {
        return source
            .as_str()
            .and_then(parse_models_source)
            .map(|models| models_value_has_takeover_markers(&models, placeholder))
            .unwrap_or(false);
    }
    entry_has_takeover_markers(value, placeholder)
}

/// The currently selected provider id (settings.json `defaultProvider`) and
/// its models.json entry, when both exist.
pub fn selected_provider_entry() -> Result<Option<(String, Value)>, AppError> {
    let Some(id) = get_default_provider_id()? else {
        return Ok(None);
    };
    let models = read_models_json_value()?;
    let entry = models
        .get("providers")
        .and_then(|p| p.get(&id))
        .filter(|e| e.is_object())
        .cloned();
    Ok(entry.map(|e| (id, e)))
}

/// Apply proxy takeover for `id`: upsert its models.json entry from
/// `settings_config` with `baseUrl` -> `proxy_base_url` and an inline
/// placeholder `apiKey` marker, then select it (defaultProvider/defaultModel).
/// Returns the previously selected provider id so the caller can revert
/// takeover fields on it after a hot switch.
pub fn apply_takeover_and_select(
    id: &str,
    settings_config: &Value,
    proxy_base_url: &str,
    placeholder: &str,
) -> Result<Option<String>, AppError> {
    assert_pi_compatible()?;
    ensure_writable_provider(id)?;
    let config = parse_provider_config(settings_config.clone())?;
    let default_model = resolve_default_model_id(&config);

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Pi config for write: {e}")))?;

    let previous_selection = get_default_provider_id()?;

    let existing = {
        let current = read_models_json_value()?;
        current.get("providers").and_then(|p| p.get(id)).cloned()
    };
    let mut entry = provider_to_models_json(&config, existing.as_ref());
    if let Some(obj) = entry.as_object_mut() {
        obj.insert("baseUrl".to_string(), json!(proxy_base_url));
        obj.insert("apiKey".to_string(), json!(placeholder));
    }

    let mut doc = PiModelsDocument::load()?;
    doc.set_provider(id, entry)?;
    doc.save()?;

    // Credentials stay in auth.json untouched; the proxy injects the real key.
    write_default_selection(id, default_model.as_deref())?;
    Ok(previous_selection)
}

/// Revert takeover markers on `id`'s entry and rewrite the entry from the DB
/// `settings_config` (restoring its real baseUrl and auth.json API key).
/// No-op when the entry carries no markers. OAuth credentials are never
/// touched.
pub fn revert_provider_takeover(
    id: &str,
    settings_config: &Value,
    placeholder: &str,
) -> Result<(), AppError> {
    assert_pi_compatible()?;
    ensure_writable_provider(id)?;
    let config = parse_provider_config(settings_config.clone())?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Pi config for write: {e}")))?;

    let existing = {
        let current = read_models_json_value()?;
        current.get("providers").and_then(|p| p.get(id)).cloned()
    };
    let Some(existing) = existing else {
        return Ok(());
    };
    if !entry_has_takeover_markers(&existing, placeholder) {
        return Ok(());
    }
    let mut stripped = existing.as_object().cloned().unwrap_or_default();
    if stripped.get("apiKey").and_then(|v| v.as_str()) == Some(placeholder) {
        stripped.remove("apiKey");
    }
    // Reach here only when apiKey==placeholder, which only happens after we
    // wrote the entry via `apply_takeover_and_select`. The baseUrl we wrote
    // pointed at the local proxy (loopback), so stripping a loopback baseUrl
    // restores the user's original config. User-provided loopback URLs are
    // never reached here because no marker was set.
    if stripped
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(is_loopback_url)
        .unwrap_or(false)
    {
        stripped.remove("baseUrl");
    }

    let entry = provider_to_models_json(&config, Some(&Value::Object(stripped)));
    let mut doc = PiModelsDocument::load()?;
    doc.set_provider(id, entry)?;
    doc.save()?;

    set_auth_api_key(id, config.api_key.as_deref())
}

/// Remove takeover markers from a single provider entry without a DB config
/// to restore from (provider not managed by CC Switch). Leaves the entry
/// otherwise intact; a removed local-proxy baseUrl is simply dropped.
pub fn remove_provider_takeover_markers(id: &str, placeholder: &str) -> Result<(), AppError> {
    assert_pi_compatible()?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Pi config for write: {e}")))?;

    let existing = {
        let current = read_models_json_value()?;
        current.get("providers").and_then(|p| p.get(id)).cloned()
    };
    let Some(existing) = existing else {
        return Ok(());
    };
    if !entry_has_takeover_markers(&existing, placeholder) {
        return Ok(());
    }
    let mut stripped = existing.as_object().cloned().unwrap_or_default();
    if stripped.get("apiKey").and_then(|v| v.as_str()) == Some(placeholder) {
        stripped.remove("apiKey");
    }
    if stripped
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(is_loopback_url)
        .unwrap_or(false)
    {
        stripped.remove("baseUrl");
    }

    let mut doc = PiModelsDocument::load()?;
    doc.set_provider(id, Value::Object(stripped))?;
    doc.save()
}

/// Last-resort cleanup: strip takeover markers from EVERY provider entry in
/// models.json. Returns whether anything changed. Caller must hold
/// [`write_lock`].
fn remove_takeover_markers_all_locked(placeholder: &str) -> Result<bool, AppError> {
    let models = read_models_json_value()?;
    let Some(providers) = models.get("providers").and_then(|v| v.as_object()) else {
        return Ok(false);
    };

    let mut changed: Vec<(String, Value)> = Vec::new();
    for (id, entry) in providers {
        if !entry_has_takeover_markers(entry, placeholder) {
            continue;
        }
        let mut stripped = entry.as_object().cloned().unwrap_or_default();
        if stripped.get("apiKey").and_then(|v| v.as_str()) == Some(placeholder) {
            stripped.remove("apiKey");
        }
        // See `revert_provider_takeover`: only reached when apiKey==placeholder,
        // which only happens for entries we wrote via `apply_takeover_and_select`.
        // User-provided loopback baseUrls are never classified as markers.
        if stripped
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .map(is_loopback_url)
            .unwrap_or(false)
        {
            stripped.remove("baseUrl");
        }
        changed.push((id.clone(), Value::Object(stripped)));
    }
    if changed.is_empty() {
        return Ok(false);
    }

    let mut doc = PiModelsDocument::load()?;
    for (id, entry) in &changed {
        doc.set_provider(id, entry.clone())?;
    }
    doc.save()?;
    Ok(true)
}

/// Last-resort cleanup: strip takeover markers from EVERY provider entry in
/// models.json (backup-missing restore fallback).
pub fn remove_takeover_markers_all(placeholder: &str) -> Result<bool, AppError> {
    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Pi config for write: {e}")))?;
    remove_takeover_markers_all_locked(placeholder)
}

/// Patch a backup snapshot with a provider's pristine (non-takeover) entry:
/// replaces `providers.<id>` inside `modelsSource` (JSONC round-trip, so
/// comments and sibling entries survive), refreshes `authApiKeys`, and points
/// the selection at the provider. Used to keep the restore backup aligned
/// with hot switches and provider edits during takeover.
pub fn patch_snapshot_provider(
    snapshot: &mut Value,
    id: &str,
    settings_config: &Value,
) -> Result<(), AppError> {
    let config = parse_provider_config(settings_config.clone())?;

    let source = snapshot
        .get("modelsSource")
        .and_then(|v| v.as_str())
        .unwrap_or("{}\n")
        .to_string();
    let text = rt_from_str(&source).map_err(|e| {
        AppError::localized(
            "provider.pi.models.invalid",
            format!("Pi models.json 备份解析失败: {}", e.message),
            format!("Failed to parse Pi backup models.json: {}", e.message),
        )
    })?;
    let mut doc = PiModelsDocument {
        path: get_pi_models_path(),
        original_source: None, // in-memory only; save() is not used here
        text,
    };

    let models_value = parse_models_source(&source).unwrap_or_else(|| json!({}));
    let existing = models_value
        .get("providers")
        .and_then(|p| p.get(id))
        .cloned();
    let entry = provider_to_models_json(&config, existing.as_ref());
    doc.set_provider(id, entry)?;

    snapshot["modelsSource"] = json!(doc.text.to_string());

    let mut keys = snapshot
        .get("authApiKeys")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    match config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(key) => {
            keys.insert(id.to_string(), json!(key));
        }
        None => {
            keys.remove(id);
        }
    }
    snapshot["authApiKeys"] = Value::Object(keys);

    let mut settings = snapshot
        .get("settings")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    settings.insert("defaultProvider".to_string(), json!(id));
    match resolve_default_model_id(&config) {
        Some(model) => {
            settings.insert("defaultModel".to_string(), json!(model));
        }
        None => {
            settings.remove("defaultModel");
        }
    }
    snapshot["settings"] = Value::Object(settings);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home<F: FnOnce(PathBuf)>(f: F) {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("pi-agent");
        fs::create_dir_all(&home).unwrap();
        std::env::set_var("PI_CODING_AGENT_DIR", &home);
        f(home);
        std::env::remove_var("PI_CODING_AGENT_DIR");
    }

    fn sample_config() -> PiProviderConfig {
        PiProviderConfig {
            r#type: "openai-completions".into(),
            api_key: Some("sk-test".into()),
            base_url: Some("http://localhost:11434/v1".into()),
            models: vec![PiModel {
                id: "llama3.1:8b".into(),
                name: Some("Llama 3.1".into()),
                reasoning: Some(false),
                input: Some(vec!["text".into()]),
                context_window: Some(128_000),
                max_tokens: Some(4096),
                ..Default::default()
            }],
            default_model_id: Some("llama3.1:8b".into()),
            ..Default::default()
        }
    }

    #[test]
    #[serial]
    fn set_and_get_provider_roundtrip() {
        with_temp_home(|_home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("local-ollama", value).unwrap();

            let providers = get_providers().unwrap();
            let entry = providers.get("local-ollama").unwrap();
            assert_eq!(entry["type"], "openai-completions");
            assert_eq!(entry["baseUrl"], "http://localhost:11434/v1");
            assert_eq!(entry["apiKey"], "sk-test");
            assert_eq!(entry["models"][0]["id"], "llama3.1:8b");
            assert_eq!(entry["models"][0]["contextWindow"], 128_000);

            assert_eq!(
                get_default_provider_id().unwrap().as_deref(),
                Some("local-ollama")
            );
            assert_eq!(get_default_model().unwrap().as_deref(), Some("llama3.1:8b"));
        });
    }

    #[test]
    #[serial]
    fn writes_auth_key_and_preserves_oauth() {
        with_temp_home(|home| {
            let _write_guard = write_lock().lock().unwrap();
            // Pre-seed an OAuth credential owned by Pi for another provider.
            fs::write(
                home.join("auth.json"),
                r#"{ "anthropic": { "type": "oauth", "refresh": "r", "access": "a", "expires": 1 } }"#,
            )
            .unwrap();

            // Writing a provider whose auth.json entry is OAuth must not clobber it.
            set_auth_api_key("anthropic", Some("sk-should-not-win")).unwrap();
            let auth = read_json_file(&home.join("auth.json")).unwrap();
            assert_eq!(auth["anthropic"]["type"], "oauth");
            assert_eq!(auth["anthropic"]["refresh"], "r");

            // API key round-trip for a normal provider.
            set_auth_api_key("local", Some("sk-local")).unwrap();
            let auth = read_json_file(&home.join("auth.json")).unwrap();
            assert_eq!(auth["local"]["key"], "sk-local");
            assert_eq!(auth["anthropic"]["type"], "oauth");

            // Removal only touches api_key entries.
            remove_auth_api_key("anthropic").unwrap();
            let auth = read_json_file(&home.join("auth.json")).unwrap();
            assert_eq!(auth["anthropic"]["type"], "oauth");
            remove_auth_api_key("local").unwrap();
            let auth = read_json_file(&home.join("auth.json")).unwrap();
            assert!(auth.get("local").is_none());
        });
    }

    #[test]
    #[serial]
    fn models_json_edit_preserves_comments_and_unknown_keys() {
        with_temp_home(|home| {
            fs::write(
                home.join("models.json"),
                "{\n  // user comment\n  \"providers\": {\n    \"hand-written\": {\n      \"baseUrl\": \"https://example.com\",\n      \"oauth\": \"radius\",\n      \"apiKey\": \"$HAND_KEY\"\n    }\n  },\n  \"otherTopLevel\": true\n}\n",
            )
            .unwrap();

            let value = serde_json::to_value(sample_config()).unwrap();
            set_provider("local-ollama", value).unwrap();

            let text = fs::read_to_string(home.join("models.json")).unwrap();
            assert!(
                text.contains("// user comment"),
                "comment preserved: {text}"
            );
            assert!(
                text.contains("\"otherTopLevel\": true"),
                "unknown key preserved: {text}"
            );
            assert!(
                text.contains("hand-written"),
                "sibling provider preserved: {text}"
            );
            assert!(
                text.contains("local-ollama"),
                "new provider inserted: {text}"
            );

            // Update in place: unknown keys of the managed entry survive, owned keys replaced.
            let mut config = sample_config();
            config.base_url = Some("http://localhost:9999/v1".into());
            let value = serde_json::to_value(&config).unwrap();
            set_provider("hand-written", value).unwrap();
            let text = fs::read_to_string(home.join("models.json")).unwrap();
            assert!(text.contains("localhost:9999"), "owned key updated: {text}");
            assert!(
                text.contains("\"oauth\": \"radius\""),
                "unowned key preserved: {text}"
            );
            assert!(
                text.contains("\"apiKey\": \"$HAND_KEY\""),
                "hand apiKey preserved: {text}"
            );
        });
    }

    #[test]
    #[serial]
    fn remove_provider_clears_selection_and_keeps_valid_json() {
        with_temp_home(|home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("tmp", value.clone()).unwrap();
            set_provider("second", value).unwrap();

            // Remove the LAST provider entry; models.json must stay strict-JSON valid.
            remove_provider("second").unwrap();
            let text = fs::read_to_string(home.join("models.json")).unwrap();
            let parsed: Value = serde_json::from_str(&text).unwrap_or_else(|e| {
                panic!("models.json must stay strict JSON after removal: {e}\n{text}")
            });
            assert!(parsed["providers"].get("second").is_none());
            assert!(parsed["providers"].get("tmp").is_some());

            remove_provider("tmp").unwrap();
            assert!(get_providers().unwrap().is_empty());
            assert_eq!(get_default_provider_id().unwrap(), None);
            assert_eq!(get_default_model().unwrap(), None);

            let auth = read_json_file(&get_pi_auth_path()).unwrap();
            assert!(auth.get("tmp").is_none());
        });
    }

    #[test]
    #[serial]
    fn managed_provider_is_readonly() {
        with_temp_home(|_home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            assert!(set_provider("managed:anthropic", value.clone()).is_err());
            assert!(remove_provider("managed:anthropic").is_err());
        });
    }

    #[test]
    #[serial]
    fn version_parse() {
        assert_eq!(parse_version("0.82.0"), Some((0, 82, 0)));
        assert_eq!(parse_version("v0.52.7"), Some((0, 52, 7)));
        assert_eq!(parse_version("1.0.0-beta.3"), Some((1, 0, 0)));
        assert_eq!(parse_version("garbage"), None);
    }

    #[test]
    #[serial]
    fn takeover_snapshot_roundtrip_restores_verbatim() {
        with_temp_home(|home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("local-ollama", value.clone()).unwrap();
            set_provider("second", value).unwrap();

            let before = fs::read_to_string(home.join("models.json")).unwrap();
            let snapshot = read_live_snapshot().unwrap();
            assert_eq!(snapshot["modelsSource"].as_str().unwrap(), before);
            assert_eq!(
                snapshot["authApiKeys"]["local-ollama"].as_str().unwrap(),
                "sk-test"
            );

            apply_takeover_and_select(
                "local-ollama",
                &serde_json::to_value(sample_config()).unwrap(),
                "http://127.0.0.1:15721/pi",
                "PROXY_MANAGED",
            )
            .unwrap();

            let models = read_models_json_value().unwrap();
            let entry = &models["providers"]["local-ollama"];
            assert_eq!(entry["baseUrl"], "http://127.0.0.1:15721/pi");
            assert_eq!(entry["apiKey"], "PROXY_MANAGED");
            // Pre-takeover snapshot (no markers): has_takeover_markers == false.
            assert!(!has_takeover_markers(&snapshot, "PROXY_MANAGED"));
            // Post-takeover snapshot (placeholder apiKey present): detected.
            let taken_over = read_live_snapshot().unwrap();
            assert!(has_takeover_markers(&taken_over, "PROXY_MANAGED"));

            write_live_snapshot(&snapshot, "PROXY_MANAGED").unwrap();
            let after = fs::read_to_string(home.join("models.json")).unwrap();
            assert_eq!(after, before, "restore must be verbatim");
        });
    }

    #[test]
    #[serial]
    fn takeover_revert_restores_db_config_and_keeps_oauth_untouched() {
        with_temp_home(|home| {
            // OAuth credential for the provider must survive takeover + revert.
            fs::write(
                home.join("auth.json"),
                r#"{ "local-ollama": { "type": "oauth", "refresh": "r", "access": "a", "expires": 1 } }"#,
            )
            .unwrap();
            let mut config = sample_config();
            config.api_key = None; // credential is OAuth-only
            let value = serde_json::to_value(&config).unwrap();
            upsert_and_select("local-ollama", value.clone()).unwrap();

            apply_takeover_and_select(
                "local-ollama",
                &value,
                "http://127.0.0.1:15721/pi",
                "PROXY_MANAGED",
            )
            .unwrap();
            let auth = read_json_file(&home.join("auth.json")).unwrap();
            assert_eq!(auth["local-ollama"]["type"], "oauth");
            assert_eq!(auth["local-ollama"]["refresh"], "r");

            revert_provider_takeover("local-ollama", &value, "PROXY_MANAGED").unwrap();
            let models = read_models_json_value().unwrap();
            let entry = &models["providers"]["local-ollama"];
            assert_eq!(entry["baseUrl"], "http://localhost:11434/v1");
            assert!(entry.get("apiKey").is_none());
            let auth = read_json_file(&home.join("auth.json")).unwrap();
            assert_eq!(auth["local-ollama"]["type"], "oauth");
        });
    }

    #[test]
    #[serial]
    fn remove_takeover_markers_all_strips_every_marked_entry() {
        with_temp_home(|_home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("local-ollama", value.clone()).unwrap();
            set_provider("second", value.clone()).unwrap();
            apply_takeover_and_select(
                "local-ollama",
                &value,
                "http://127.0.0.1:15721/pi",
                "PROXY_MANAGED",
            )
            .unwrap();
            apply_takeover_and_select(
                "second",
                &value,
                "http://127.0.0.1:15721/pi",
                "PROXY_MANAGED",
            )
            .unwrap();

            let changed = remove_takeover_markers_all("PROXY_MANAGED").unwrap();
            assert!(changed);
            let snapshot = read_live_snapshot().unwrap();
            assert!(!has_takeover_markers(&snapshot, "PROXY_MANAGED"));
            let models = read_models_json_value().unwrap();
            // Entries survive; only markers are stripped.
            assert!(models["providers"].get("local-ollama").is_some());
            assert!(models["providers"].get("second").is_some());
        });
    }

    /// Regression test for the defect fixed alongside this test: prior to
    /// the fix, `entry_has_takeover_markers` returned true on ANY loopback
    /// baseUrl regardless of apiKey. This had two visible user-facing
    /// consequences:
    ///
    /// 1. `backup_live_configs` would skip saving a backup ("live taken over")
    ///    even though the user's Ollama/LM Studio/custom-loopback provider
    ///    was never actually taken over. On the next start-with-takeover
    ///    cycle, restore would have no source of truth and fall to SSOT
    ///    rebuild (overwriting the user's local model URL with the SSOT
    ///    provider URL).
    ///
    /// 2. `remove_takeover_markers_all` would silently strip `baseUrl` from
    ///    the user's local provider entries, leaving them with no explicit
    ///    upstream and forcing Pi's resolver to fall back to its built-in
    ///    default endpoint for the chosen provider shape (`api.anthropic.com`
    ///    for Anthropic-shaped, `api.openai.com` for OpenAI-shaped).
    ///
    /// Asserts:
    /// - `has_takeover_markers(snapshot)` returns false for a snapshot whose
    ///   only possible-source-of-confusion is a user-authored loopback
    ///   baseUrl on a provider the user added.
    /// - `remove_takeover_markers_all` reports no change and leaves both the
    ///   user's baseUrl and `auth.json` apiKey entry untouched.
    #[test]
    #[serial]
    fn local_proxy_loopback_baseurl_alone_is_not_a_takeover_marker() {
        with_temp_home(|home| {
            // Two providers written via the normal API: one Ollama-shaped
            // (loopback baseUrl, identical host shape to the proxy) and one
            // upstream relay (also loopback in this fixture, but otherwise
            // innocuous). Neither has had `apply_takeover_and_select` called
            // against it, so neither has the placeholder apiKey.
            upsert_and_select(
                "user-ollama",
                serde_json::to_value(sample_config()).unwrap(),
            )
            .unwrap();
            set_provider("user-relay", serde_json::to_value(sample_config()).unwrap()).unwrap();

            let snapshot = read_live_snapshot().unwrap();
            // Without markers applied, the snapshot is "not taken over".
            assert!(
                !has_takeover_markers(&snapshot, "PROXY_MANAGED"),
                "user's loopback baseUrl must not be misread as a takeover marker (regression)"
            );

            // Last-resort cleanup must be a no-op: the user's baseUrl and
            // auth.json API key survive, and the cleanup returns false
            // (nothing changed).
            let changed = remove_takeover_markers_all("PROXY_MANAGED").unwrap();
            assert!(!changed, "no entries should be touched");

            // models.json: both baseUrls preserved verbatim.
            let models_after = read_models_json_value().unwrap();
            assert_eq!(
                models_after["providers"]["user-ollama"]["baseUrl"],
                "http://localhost:11434/v1"
            );
            assert_eq!(
                models_after["providers"]["user-relay"]["baseUrl"],
                "http://localhost:11434/v1"
            );

            // auth.json: api_key entries for both providers are intact
            // (cleanup never touches auth.json, only models.json markers).
            let auth = read_json_file(&home.join("auth.json")).unwrap();
            assert_eq!(auth["user-ollama"]["type"], "api_key");
            assert_eq!(auth["user-ollama"]["key"], "sk-test");
            assert_eq!(auth["user-relay"]["type"], "api_key");
            assert_eq!(auth["user-relay"]["key"], "sk-test");
        });
    }

    #[test]
    #[serial]
    fn patch_snapshot_provider_updates_entry_and_selection() {
        with_temp_home(|_home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("local-ollama", value.clone()).unwrap();
            set_provider("second", value).unwrap();

            let mut snapshot = read_live_snapshot().unwrap();
            let mut edited = sample_config();
            edited.base_url = Some("https://edited.example.com/v1".into());
            edited.api_key = Some("sk-edited".into());
            let edited_value = serde_json::to_value(&edited).unwrap();
            patch_snapshot_provider(&mut snapshot, "second", &edited_value).unwrap();

            let models = parse_models_source(snapshot["modelsSource"].as_str().unwrap()).unwrap();
            assert_eq!(
                models["providers"]["second"]["baseUrl"],
                "https://edited.example.com/v1"
            );
            assert!(models["providers"].get("local-ollama").is_some());
            assert_eq!(snapshot["authApiKeys"]["second"], "sk-edited");
            assert_eq!(snapshot["settings"]["defaultProvider"], "second");
            assert_eq!(snapshot["settings"]["defaultModel"], "llama3.1:8b");
        });
    }

    #[test]
    #[serial]
    fn takeover_requires_existing_writable_provider_id() {
        with_temp_home(|_home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            assert!(apply_takeover_and_select(
                "managed:anthropic",
                &value,
                "http://127.0.0.1:15721/pi",
                "PROXY_MANAGED",
            )
            .is_err());
        });
    }
}
