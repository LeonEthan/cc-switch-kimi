//! Omp (Oh My Pi) coding agent configuration (`~/.omp/agent`).
//!
//! Omp is a fork of Pi; this module mirrors `pi_config.rs`'s discipline but
//! adapts it to omp's YAML config layout:
//! - All providers coexist under the top-level `providers:` mapping in
//!   `models.yml` (YAML, not JSONC). Custom providers carry their API key
//!   inline (`apiKey`), which omp resolves with higher priority than stored
//!   OAuth credentials.
//! - Active selection is **role-based**: `modelRoles` in `config.yml` maps
//!   each role (`default` / `smol` / `slow` / `plan` / `commit`) to a
//!   `<providerKey>/<modelId>` selector. One provider may serve several
//!   roles at once.
//! - OAuth/login credentials live in `agent.db` (SQLite, table
//!   `auth_credentials`). The database is omp-owned: CC Switch only ever
//!   opens it READ-ONLY and never writes to it.
//!
//! Non-destructive coexistence: writes to `models.yml` locate the top-level
//! `providers:` section, parse ONLY that section, upsert/remove entries, and
//! splice the re-serialized section back into the raw text, so everything
//! outside the section (comments, other keys, formatting) stays
//! byte-identical. Entries written by CC Switch are stamped with
//! `_ccSource: managed`; hand-written user entries are never removed.
//! `config.yml` writes only ever touch the top-level `modelRoles` key via
//! the same section surgery. All writes validate the installed omp version
//! first (compatibility gate) and use optimistic concurrency (no clobbering
//! of concurrent edits).
//!
//! Skills live under `~/.omp/agent/skills/`, sessions under
//! `~/.omp/agent/sessions/`. Override the agent directory with
//! `PI_CODING_AGENT_DIR` or CC Switch settings (`ompConfigDir`).

use crate::config::{atomic_write, get_home_dir};
use crate::error::AppError;
use crate::hermes_config::{find_yaml_section_range, remove_all_sections, replace_yaml_section};
use crate::settings::get_omp_override_dir;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// ============================================================================
// Compatibility gate
// ============================================================================

/// Minimum omp version whose `models.yml` merge semantics and session format
/// match what this module writes.
pub const MIN_OMP_VERSION: &str = "17.0.0";

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

/// Extract the version from `omp --version` output. omp prints the slash
/// form (`omp/17.2.2`); a bare `17.2.2` is accepted too.
fn extract_omp_version(text: &str) -> Option<String> {
    let candidate = text.trim().rsplit('/').next()?.trim();
    let (major, minor, patch) = parse_version(candidate)?;
    Some(format!("{major}.{minor}.{patch}"))
}

/// Detect the installed omp version via `omp --version` (cached for the
/// process lifetime). Returns `None` when the binary is not installed or not
/// runnable; an unknown version does not block writes (the config files
/// remain valid for an omp installed later).
pub fn detect_omp_version() -> Option<String> {
    static VERSION: OnceLock<Option<String>> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            let output = std::process::Command::new("omp")
                .arg("--version")
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let text = String::from_utf8_lossy(&output.stdout);
            extract_omp_version(&text)
        })
        .clone()
}

/// Compatibility gate: refuse unsafe writes when the installed omp version
/// is known to be older than [`MIN_OMP_VERSION`]. An unknown version does
/// not block.
pub fn assert_omp_compatible() -> Result<(), AppError> {
    if let Some(version) = detect_omp_version() {
        if let (Some(found), Some(min)) = (parse_version(&version), parse_version(MIN_OMP_VERSION))
        {
            if found < min {
                return Err(AppError::localized(
                    "provider.omp.incompatible_version",
                    format!(
                        "检测到 Omp 版本 {version} 低于最低支持版本 {MIN_OMP_VERSION}，已阻止写入配置。请升级 Omp 后重试。"
                    ),
                    format!(
                        "Omp version {version} is below the minimum supported {MIN_OMP_VERSION}; config writes are blocked. Please upgrade Omp."
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

/// Resolve the omp agent directory.
///
/// Priority:
/// 1. CC Switch settings override (`ompConfigDir`)
/// 2. `PI_CODING_AGENT_DIR` environment variable (non-empty after trim, `~` expanded)
/// 3. Platform default `~/.omp/agent`
pub fn get_omp_dir() -> PathBuf {
    if let Some(override_dir) = get_omp_override_dir() {
        return override_dir;
    }

    if let Some(raw) = std::env::var_os("PI_CODING_AGENT_DIR") {
        let value = raw.to_string_lossy();
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return expand_tilde(trimmed);
        }
    }

    get_home_dir().join(".omp").join("agent")
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

/// Path of the custom-provider file (`models.yml`).
pub fn get_omp_models_path() -> PathBuf {
    get_omp_dir().join("models.yml")
}

/// Path of the main config file (`config.yml`, holds `modelRoles`).
pub fn get_omp_config_path() -> PathBuf {
    get_omp_dir().join("config.yml")
}

/// Path of the omp-owned credential database (READ-ONLY for CC Switch).
pub fn get_omp_auth_db_path() -> PathBuf {
    get_omp_dir().join("agent.db")
}

pub fn get_omp_skills_dir() -> PathBuf {
    get_omp_dir().join("skills")
}

/// Resolve the omp sessions directory (`<agent dir>/sessions`).
pub fn get_omp_sessions_dir() -> PathBuf {
    get_omp_dir().join("sessions")
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

/// The entry field marking a `models.yml` provider as CC Switch-managed.
///
/// omp uses an entry FIELD instead of Pi's `managed:` id prefix: YAML keys
/// containing `:` are a parsing hazard, and the role selector
/// `<key>/<modelId>` forbids both `:` and `/` in provider keys.
pub const MANAGED_SOURCE_FIELD: &str = "_ccSource";

/// Whether a `models.yml` provider entry is CC Switch-managed.
pub fn is_managed_provider(entry: &Value) -> bool {
    entry
        .as_object()
        .and_then(|obj| obj.get(MANAGED_SOURCE_FIELD))
        .and_then(|v| v.as_str())
        == Some(PROVIDER_SOURCE_MANAGED)
}

/// Provider key validation: `^[a-z0-9][a-z0-9._-]*$`.
///
/// `:` and `/` are forbidden so the `<key>/<modelId>` role selector stays
/// parseable.
pub fn validate_provider_key(id: &str) -> Result<(), AppError> {
    let bytes = id.as_bytes();
    let valid = !bytes.is_empty()
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'));
    if !valid {
        return Err(AppError::localized(
            "provider.omp.invalid_key",
            format!(
                "Omp 供应商标识 '{id}' 无效：必须匹配 ^[a-z0-9][a-z0-9._-]*$（不得包含 ':' 或 '/'）"
            ),
            format!(
                "Invalid Omp provider key '{id}': must match ^[a-z0-9][a-z0-9._-]*$ (no ':' or '/')"
            ),
        ));
    }
    Ok(())
}

/// The five omp model roles.
pub const OMP_ROLES: [&str; 5] = ["default", "smol", "slow", "plan", "commit"];

/// Role validation: only `default` / `smol` / `slow` / `plan` / `commit`.
pub fn validate_role(role: &str) -> Result<(), AppError> {
    if !OMP_ROLES.contains(&role) {
        return Err(AppError::localized(
            "provider.omp.invalid_role",
            format!("Omp 模型角色 '{role}' 无效。可选值: default, smol, slow, plan, commit。"),
            format!("Invalid Omp model role '{role}'. Allowed: default, smol, slow, plan, commit."),
        ));
    }
    Ok(())
}

fn default_omp_api() -> String {
    "openai-completions".to_string()
}

/// Single model entry stored in CC Switch `settingsConfig.models[]`.
/// Mirrors omp's `models.yml` model definition (camelCase on the wire).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OmpModel {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Per-model API override (defaults to the provider's `api`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
}

/// Provider fragment stored as `settingsConfig` in the database.
///
/// The protocol field is canonically `api` (`openai-completions`,
/// `anthropic-messages`, …); the legacy alias `type` is accepted on read for
/// preset symmetry with Pi.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OmpProviderConfig {
    /// API protocol: `openai-completions`, `openai-responses`,
    /// `openai-codex-responses`, `azure-openai-responses`,
    /// `anthropic-messages`, `google-generative-ai`, `google-vertex`.
    #[serde(default = "default_omp_api", alias = "type")]
    pub api: String,
    /// Inline API key (literal or environment-variable name, per omp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<OmpModel>,
    /// Preferred model id used in the `<key>/<modelId>` role selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model_id: Option<String>,
    /// Human-readable provider name (models.yml `name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// models.yml `authHeader`: add `Authorization: Bearer <apiKey>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_header: Option<bool>,
    /// Internal marker for providers imported from live (`user` / `oauth` / `managed`).
    /// Explicit rename: serde's `camelCase` rule drops the leading underscore
    /// (`_cc_source` -> `ccSource`), but the DB/frontend contract is `_ccSource`.
    #[serde(
        rename = "_ccSource",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub _cc_source: Option<String>,
}

impl Default for OmpProviderConfig {
    fn default() -> Self {
        Self {
            api: default_omp_api(),
            api_key: None,
            base_url: None,
            models: Vec::new(),
            default_model_id: None,
            display_name: None,
            auth_header: None,
            _cc_source: None,
        }
    }
}

fn parse_provider_config(settings_config: Value) -> Result<OmpProviderConfig, AppError> {
    serde_json::from_value(settings_config).map_err(|e| {
        AppError::localized(
            "provider.omp.config.invalid",
            format!("Omp 供应商配置无效: {e}"),
            format!("Invalid Omp provider config: {e}"),
        )
    })
}

// ============================================================================
// YAML section surgery helpers (models.yml `providers:` / config.yml `modelRoles:`)
// ============================================================================

/// Parse the top-level `providers:` section of a `models.yml` raw text into a
/// JSON object map (id → entry). Missing section yields an empty map; a
/// section that cannot be parsed as YAML, or whose value is not a mapping,
/// is an error (never silently corrupt).
fn parse_providers_section(raw: &str) -> Result<Map<String, Value>, AppError> {
    let Some((start, end)) = find_yaml_section_range(raw, "providers") else {
        return Ok(Map::new());
    };
    let section: serde_yaml::Value =
        serde_yaml::from_str(&raw[start..end]).map_err(|e| {
            AppError::localized(
                "provider.omp.models.invalid",
                format!("Omp models.yml 的 providers 区段解析失败: {e}"),
                format!("Failed to parse the providers section of Omp models.yml: {e}"),
            )
        })?;
    let Some(mapping) = section.get("providers").and_then(|v| v.as_mapping()) else {
        return Err(AppError::localized(
            "provider.omp.models.invalid",
            "Omp models.yml 的 providers 区段必须是 YAML 映射",
            "The providers section of Omp models.yml must be a YAML mapping",
        ));
    };
    yaml_mapping_to_json_map(mapping, "provider.omp.models.invalid")
}

/// Parse the top-level `modelRoles:` section of a `config.yml` raw text into
/// a YAML mapping. Missing section yields an empty mapping.
fn parse_model_roles_section(raw: &str) -> Result<serde_yaml::Mapping, AppError> {
    let Some((start, end)) = find_yaml_section_range(raw, "modelRoles") else {
        return Ok(serde_yaml::Mapping::new());
    };
    let section: serde_yaml::Value =
        serde_yaml::from_str(&raw[start..end]).map_err(|e| {
            AppError::localized(
                "provider.omp.config.invalid",
                format!("Omp config.yml 的 modelRoles 区段解析失败: {e}"),
                format!("Failed to parse the modelRoles section of Omp config.yml: {e}"),
            )
        })?;
    section
        .get("modelRoles")
        .and_then(|v| v.as_mapping())
        .cloned()
        .ok_or_else(|| {
            AppError::localized(
                "provider.omp.config.invalid",
                "Omp config.yml 的 modelRoles 区段必须是 YAML 映射",
                "The modelRoles section of Omp config.yml must be a YAML mapping",
            )
        })
}

fn yaml_mapping_to_json_map(
    mapping: &serde_yaml::Mapping,
    error_key: &'static str,
) -> Result<Map<String, Value>, AppError> {
    match serde_json::to_value(serde_yaml::Value::Mapping(mapping.clone())) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(AppError::localized(
            error_key,
            "Omp YAML 区段包含无法转换为 JSON 的内容（如非字符串键）",
            "Omp YAML section contains content not convertible to JSON (e.g. non-string keys)",
        )),
    }
}

fn json_map_to_yaml_mapping(map: &Map<String, Value>) -> Result<serde_yaml::Mapping, AppError> {
    match serde_yaml::to_value(Value::Object(map.clone())) {
        Ok(serde_yaml::Value::Mapping(mapping)) => Ok(mapping),
        _ => Err(AppError::Message(
            "Failed to convert Omp providers to YAML".to_string(),
        )),
    }
}

/// Serialize the providers map and splice it back into `raw`, keeping every
/// byte outside the `providers:` section untouched. An empty map removes the
/// section entirely.
fn splice_providers_section(raw: &str, providers: &Map<String, Value>) -> Result<String, AppError> {
    if providers.is_empty() {
        return Ok(remove_all_sections(raw, "providers"));
    }
    let mapping = json_map_to_yaml_mapping(providers)?;
    replace_yaml_section(raw, "providers", &serde_yaml::Value::Mapping(mapping))
}

// ============================================================================
// models.yml document (optimistic-concurrency round-trip)
// ============================================================================

/// A parsed `models.yml` document. Only the `providers.<id>` entries are ever
/// mutated; everything outside the `providers:` section is preserved
/// byte-for-byte.
struct OmpModelsDocument {
    path: PathBuf,
    original_source: Option<String>,
    providers: Map<String, Value>,
}

impl OmpModelsDocument {
    fn load() -> Result<Self, AppError> {
        let path = get_omp_models_path();
        let original_source = if path.exists() {
            Some(fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?)
        } else {
            None
        };
        let providers = match original_source.as_deref() {
            Some(raw) if !raw.trim().is_empty() => parse_providers_section(raw)?,
            _ => Map::new(),
        };
        Ok(Self {
            path,
            original_source,
            providers,
        })
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
                "provider.omp.models.changed_on_disk",
                "Omp models.yml 在磁盘上已被修改，请重试",
                "Omp models.yml changed on disk; please retry",
            ));
        }
        let raw = self.original_source.clone().unwrap_or_default();
        let next_source = splice_providers_section(&raw, &self.providers)?;
        if next_source.trim().is_empty() && current_source.is_none() {
            return Ok(()); // nothing to write; don't create an empty file
        }
        if current_source.as_deref() == Some(next_source.as_str()) {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }
        atomic_write(&self.path, next_source.as_bytes())?;
        // models.yml carries inline API keys — owner-only permissions.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600));
        }
        log::debug!("Omp models.yml written to {:?}", self.path);
        Ok(())
    }
}

/// Mutate `config.yml`'s top-level `modelRoles` mapping with optimistic
/// concurrency. Only that key is ever replaced/inserted/removed; every other
/// key stays byte-identical. The closure returns whether it changed anything.
/// Caller must hold [`write_lock`].
fn mutate_model_roles_locked<F>(mutate: F) -> Result<(), AppError>
where
    F: FnOnce(&mut serde_yaml::Mapping) -> Result<bool, AppError>,
{
    let path = get_omp_config_path();
    let original_source = if path.exists() {
        Some(fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?)
    } else {
        None
    };
    let raw = original_source.clone().unwrap_or_default();
    let mut roles = if raw.trim().is_empty() {
        serde_yaml::Mapping::new()
    } else {
        parse_model_roles_section(&raw)?
    };

    if !mutate(&mut roles)? {
        return Ok(());
    }

    let next_source = if roles.is_empty() {
        remove_all_sections(&raw, "modelRoles")
    } else {
        replace_yaml_section(&raw, "modelRoles", &serde_yaml::Value::Mapping(roles))?
    };

    let current_source = if path.exists() {
        Some(fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?)
    } else {
        None
    };
    if current_source != original_source {
        return Err(AppError::localized(
            "provider.omp.config.changed_on_disk",
            "Omp config.yml 在磁盘上已被修改，请重试",
            "Omp config.yml changed on disk; please retry",
        ));
    }
    if next_source.trim().is_empty() && current_source.is_none() {
        return Ok(());
    }
    if current_source.as_deref() == Some(next_source.as_str()) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }
    atomic_write(&path, next_source.as_bytes())?;
    log::debug!("Omp config.yml written to {:?}", path);
    Ok(())
}

/// Read the live `modelRoles` mapping (role → `<key>/<modelId>` selector).
/// Only string-valued entries are returned.
pub fn get_model_roles() -> Result<Map<String, Value>, AppError> {
    let path = get_omp_config_path();
    let mut result = Map::new();
    if !path.exists() {
        return Ok(result);
    }
    let raw = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    if raw.trim().is_empty() {
        return Ok(result);
    }
    let roles = parse_model_roles_section(&raw)?;
    for (key, value) in &roles {
        if let (serde_yaml::Value::String(role), serde_yaml::Value::String(selector)) =
            (key, value)
        {
            result.insert(role.clone(), json!(selector));
        }
    }
    Ok(result)
}

// ============================================================================
// models.yml provider entry <-> OmpProviderConfig
// ============================================================================

fn model_to_entry(model: &OmpModel) -> Value {
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
    if let Some(n) = model.context_window {
        obj.insert("contextWindow".to_string(), json!(n));
    }
    if let Some(n) = model.max_tokens {
        obj.insert("maxTokens".to_string(), json!(n));
    }
    Value::Object(obj)
}

fn model_from_entry(value: &Value) -> Option<OmpModel> {
    let obj = value.as_object()?;
    let id = obj.get("id").and_then(|v| v.as_str())?.trim().to_string();
    if id.is_empty() {
        return None;
    }
    Some(OmpModel {
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
        context_window: obj.get("contextWindow").and_then(|v| v.as_u64()),
        max_tokens: obj.get("maxTokens").and_then(|v| v.as_u64()),
    })
}

/// Build the `models.yml` provider entry, preserving keys CC Switch does not
/// own (unknown extension keys) from an existing entry. The entry is stamped
/// `_ccSource: managed` so future writes can tell CC Switch-managed entries
/// apart from hand-written user entries.
fn provider_to_entry(config: &OmpProviderConfig, existing: Option<&Value>) -> Value {
    let mut obj = existing
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    // `api` is canonical; drop the legacy read alias if present.
    obj.remove("type");
    let api = config.api.trim();
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

    match config.auth_header {
        Some(v) => {
            obj.insert("authHeader".to_string(), json!(v));
        }
        None => {
            obj.remove("authHeader");
        }
    }

    // omp keeps the API key inline in models.yml (higher priority than the
    // stored OAuth credentials in agent.db).
    match config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(key) => {
            obj.insert("apiKey".to_string(), json!(key));
        }
        None => {
            obj.remove("apiKey");
        }
    }

    let models: Vec<Value> = config
        .models
        .iter()
        .filter(|m| !m.id.trim().is_empty())
        .map(model_to_entry)
        .collect();
    if models.is_empty() {
        obj.remove("models");
    } else {
        obj.insert("models".to_string(), Value::Array(models));
    }

    obj.insert(
        MANAGED_SOURCE_FIELD.to_string(),
        json!(PROVIDER_SOURCE_MANAGED),
    );

    Value::Object(obj)
}

fn provider_from_entry(entry: &Value, source: &str) -> OmpProviderConfig {
    let obj = entry.as_object();
    let get_str = |key: &str| {
        obj.and_then(|o| o.get(key))
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
    };
    let models = obj
        .and_then(|o| o.get("models"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(model_from_entry).collect())
        .unwrap_or_default();

    OmpProviderConfig {
        api: get_str("api")
            .or_else(|| get_str("type"))
            .unwrap_or_else(default_omp_api),
        api_key: get_str("apiKey"),
        base_url: get_str("baseUrl"),
        models,
        default_model_id: None,
        display_name: get_str("name"),
        auth_header: obj
            .and_then(|o| o.get("authHeader"))
            .and_then(|v| v.as_bool()),
        _cc_source: Some(source.to_string()),
    }
}

// ============================================================================
// agent.db (omp-owned credential store; READ-ONLY access only)
// ============================================================================

/// Open `agent.db` READ-ONLY and run `f`. Returns `None` when the database
/// is missing or cannot be opened. NEVER opens the database writable: omp
/// owns these credentials (OAuth tokens, login state, WAL).
fn with_auth_db_read_only<T>(f: impl FnOnce(&rusqlite::Connection) -> T) -> Option<T> {
    let path = get_omp_auth_db_path();
    if !path.exists() {
        return None;
    }
    let conn = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    Some(f(&conn))
}

fn connection_has_oauth_credential(conn: &rusqlite::Connection, provider_id: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM auth_credentials WHERE provider = ?1 AND credential_type = 'oauth' LIMIT 1",
        [provider_id],
        |_| Ok(()),
    )
    .is_ok()
}

/// Whether omp owns an OAuth credential for this provider id (`agent.db`,
/// read-only query; false when the database or table is missing).
pub fn provider_has_oauth_credential(provider_id: &str) -> bool {
    with_auth_db_read_only(|conn| connection_has_oauth_credential(conn, provider_id))
        .unwrap_or(false)
}

// ============================================================================
// Provider CRUD (additive)
// ============================================================================

/// Read all providers from live `models.yml` as a map of id → settings JSON.
///
/// Entries stamped `_ccSource: managed` are reported as `managed`; entries
/// with an omp-owned OAuth credential in `agent.db` are marked `oauth`;
/// everything else is `user`. Missing file yields an empty map.
pub fn get_providers() -> Result<Map<String, Value>, AppError> {
    let path = get_omp_models_path();
    let providers = if path.exists() {
        let raw = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
        if raw.trim().is_empty() {
            Map::new()
        } else {
            parse_providers_section(&raw)?
        }
    } else {
        Map::new()
    };

    let mut result = Map::new();
    for (provider_id, entry) in &providers {
        if !entry.is_object() {
            continue;
        }
        let source = if is_managed_provider(entry) {
            PROVIDER_SOURCE_MANAGED
        } else if with_auth_db_read_only(|conn| connection_has_oauth_credential(conn, provider_id))
            .unwrap_or(false)
        {
            PROVIDER_SOURCE_OAUTH
        } else {
            PROVIDER_SOURCE_USER
        };

        let config = provider_from_entry(entry, source);
        match serde_json::to_value(config) {
            Ok(value) => {
                result.insert(provider_id.clone(), value);
            }
            Err(e) => {
                log::warn!("Failed to serialize Omp provider '{provider_id}': {e}");
            }
        }
    }

    Ok(result)
}

/// Shared upsert path: write `models.yml` `providers.<id>`.
/// Caller must hold [`write_lock`].
fn upsert_provider_locked(id: &str, config: &OmpProviderConfig) -> Result<(), AppError> {
    let mut doc = OmpModelsDocument::load()?;
    let entry = provider_to_entry(config, doc.providers.get(id));
    doc.providers.insert(id.to_string(), entry);
    doc.save()
}

/// Upsert a provider into live config (additive). Does **not** change any
/// role selection — use [`upsert_and_select`] when switching.
pub fn set_provider(id: &str, settings_config: Value) -> Result<(), AppError> {
    assert_omp_compatible()?;
    validate_provider_key(id)?;
    let config = parse_provider_config(settings_config)?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Omp config for write: {e}")))?;

    upsert_provider_locked(id, &config)
}

/// Upsert provider + assign it to `role` (`modelRoles.<role>` in
/// `config.yml`, selector `<id>/<defaultModelId>`).
pub fn upsert_and_select(id: &str, settings_config: Value, role: &str) -> Result<(), AppError> {
    assert_omp_compatible()?;
    validate_provider_key(id)?;
    validate_role(role)?;
    let config = parse_provider_config(settings_config)?;
    let default_model = resolve_default_model_id(&config);

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Omp config for write: {e}")))?;

    upsert_provider_locked(id, &config)?;

    match default_model.as_deref() {
        Some(model) => set_model_role_locked(role, id, model),
        None => Err(AppError::localized(
            "provider.omp.no_models",
            format!("Omp 供应商 '{id}' 没有可用模型，无法生成角色选择器"),
            format!("Omp provider '{id}' has no models; cannot build a role selector"),
        )),
    }
}

fn resolve_default_model_id(config: &OmpProviderConfig) -> Option<String> {
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

/// Write `modelRoles.<role> = "<provider_id>/<model_id>"`, preserving every
/// other config.yml key. Caller must hold [`write_lock`].
fn set_model_role_locked(role: &str, provider_id: &str, model_id: &str) -> Result<(), AppError> {
    let selector = format!("{provider_id}/{model_id}");
    mutate_model_roles_locked(|roles| {
        let existing = roles
            .get(serde_yaml::Value::String(role.to_string()))
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        if existing.as_deref() == Some(selector.as_str()) {
            return Ok(false);
        }
        roles.insert(
            serde_yaml::Value::String(role.to_string()),
            serde_yaml::Value::String(selector),
        );
        Ok(true)
    })
}

/// Assign `provider_id` to `role`. The selector model is the provider's first
/// model in live `models.yml` (DB-driven switches carry an explicit
/// `defaultModelId` via [`upsert_and_select`]).
#[allow(dead_code)] // Public live-config API; DB-driven flows use `upsert_and_select`.
pub fn set_model_role(role: &str, provider_id: &str) -> Result<(), AppError> {
    assert_omp_compatible()?;
    validate_role(role)?;
    validate_provider_key(provider_id)?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Omp config for write: {e}")))?;

    let doc = OmpModelsDocument::load()?;
    let entry = doc.providers.get(provider_id).ok_or_else(|| {
        AppError::localized(
            "provider.omp.provider_not_found",
            format!("Omp 供应商 '{provider_id}' 不存在于 models.yml"),
            format!("Omp provider '{provider_id}' not found in models.yml"),
        )
    })?;
    let model = entry
        .get("models")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|m| m.get("id"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            AppError::localized(
                "provider.omp.no_models",
                format!("Omp 供应商 '{provider_id}' 没有可用模型，无法生成角色选择器"),
                format!("Omp provider '{provider_id}' has no models; cannot build a role selector"),
            )
        })?;

    set_model_role_locked(role, provider_id, &model)
}

/// Clear `modelRoles.<role>` (removes the whole `modelRoles` key when it
/// becomes empty).
pub fn clear_model_role(role: &str) -> Result<(), AppError> {
    validate_role(role)?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Omp config for write: {e}")))?;

    mutate_model_roles_locked(|roles| {
        Ok(roles
            .remove(serde_yaml::Value::String(role.to_string()))
            .is_some())
    })
}

/// Remove a provider from live config: drops the `models.yml` entry and
/// strips every `modelRoles` entry pointing at it. Hand-written user entries
/// with other keys are never touched.
pub fn remove_provider(id: &str) -> Result<(), AppError> {
    assert_omp_compatible()?;
    validate_provider_key(id)?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Omp config for write: {e}")))?;

    let mut doc = OmpModelsDocument::load()?;
    let existed = doc.providers.remove(id).is_some();
    if existed {
        doc.save()?;
    }

    // Strip role selectors referencing the removed provider (`<id>/<model>`).
    let prefix = format!("{id}/");
    mutate_model_roles_locked(|roles| {
        let stale: Vec<serde_yaml::Value> = roles
            .iter()
            .filter(|(_, v)| {
                v.as_str()
                    .map(|s| s.starts_with(&prefix))
                    .unwrap_or(false)
            })
            .map(|(k, _)| k.clone())
            .collect();
        for key in &stale {
            roles.remove(key);
        }
        Ok(!stale.is_empty())
    })
}

// ============================================================================
// Live settings snapshot (for "open config" / read_live_settings)
// ============================================================================

/// Return a JSON snapshot of omp's live config for diagnostics:
/// `{config: <config.yml as JSON>, models: <providers map>}`.
///
/// OAuth credentials never appear here — they live in `agent.db`, which CC
/// Switch only reads for boolean existence checks.
pub fn read_live_settings() -> Result<Value, AppError> {
    let config_path = get_omp_config_path();
    let config = if config_path.exists() {
        let raw = fs::read_to_string(&config_path).map_err(|e| AppError::io(&config_path, e))?;
        if raw.trim().is_empty() {
            json!({})
        } else {
            let yaml: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|e| {
                AppError::localized(
                    "provider.omp.config.invalid",
                    format!("Omp config.yml 解析失败: {e}"),
                    format!("Failed to parse Omp config.yml: {e}"),
                )
            })?;
            serde_json::to_value(yaml).unwrap_or_else(|_| json!({}))
        }
    } else {
        json!({})
    };

    let models_path = get_omp_models_path();
    let models = if models_path.exists() {
        let raw = fs::read_to_string(&models_path).map_err(|e| AppError::io(&models_path, e))?;
        if raw.trim().is_empty() {
            Map::new()
        } else {
            parse_providers_section(&raw)?
        }
    } else {
        Map::new()
    };

    Ok(json!({
        "config": config,
        "models": Value::Object(models),
    }))
}

// ============================================================================
// Proxy takeover helpers (CC Switch local proxy)
// ============================================================================
//
// During proxy takeover CC Switch rewrites ONLY the selected provider's entry
// in models.yml:
// - `baseUrl` -> `{proxy origin}/omp`.
// - `apiKey` -> an inline placeholder marker (detection signal, consistent
//   with other apps' takeover markers).
//
// `modelRoles` is NEVER touched: the selectors keep pointing at the same
// `<key>/<modelId>`. agent.db is NEVER touched either (omp-owned). The inline
// placeholder is safe because omp resolves the models.yml `apiKey` with
// higher priority than stored OAuth credentials, and the proxy strips client
// credentials and injects the DB-stored key either way. The placeholder is a
// *marker*, not a credential mask.

/// Live-config snapshot used for takeover backup/restore.
///
/// `modelsSource` is the raw models.yml text (verbatim, so restores keep
/// comments/formatting); `configSource` is the raw config.yml text (takeover
/// never modifies it, but the snapshot keeps it for symmetry and safe
/// restores).
pub fn read_live_snapshot() -> Result<Value, AppError> {
    let models_path = get_omp_models_path();
    let models_source = if models_path.exists() {
        Some(fs::read_to_string(&models_path).map_err(|e| AppError::io(&models_path, e))?)
    } else {
        None
    };
    let config_path = get_omp_config_path();
    let config_source = if config_path.exists() {
        Some(fs::read_to_string(&config_path).map_err(|e| AppError::io(&config_path, e))?)
    } else {
        None
    };
    Ok(json!({
        "modelsSource": models_source,
        "configSource": config_source,
    }))
}

/// Restore a snapshot verbatim (backup restore path). Atomic writes.
///
/// When the snapshot has no `modelsSource` (models.yml did not exist at
/// backup time) any takeover markers in a since-created models.yml are
/// stripped instead of deleting a file the user may have edited.
pub fn write_live_snapshot(snapshot: &Value, placeholder: &str) -> Result<(), AppError> {
    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Omp config for write: {e}")))?;

    let models_path = get_omp_models_path();
    match snapshot.get("modelsSource").and_then(|v| v.as_str()) {
        Some(source) => {
            if let Some(parent) = models_path.parent() {
                fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
            }
            atomic_write(&models_path, source.as_bytes())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&models_path, fs::Permissions::from_mode(0o600));
            }
        }
        None => {
            if models_path.exists() {
                let _ = remove_takeover_markers_all_locked(placeholder)?;
            }
        }
    }
    if let Some(source) = snapshot.get("configSource").and_then(|v| v.as_str()) {
        let config_path = get_omp_config_path();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }
        atomic_write(&config_path, source.as_bytes())?;
    }
    Ok(())
}

/// Parse the providers map from a raw models.yml source, tolerantly (missing
/// file/section or unparseable content yields an empty map — used only for
/// marker detection, never for writes).
fn parse_models_source_providers(source: &str) -> Map<String, Value> {
    if source.trim().is_empty() {
        return Map::new();
    }
    parse_providers_section(source).unwrap_or_default()
}

/// Whether a provider entry carries the proxy-takeover apiKey placeholder.
///
/// IMPORTANT: only the inline `apiKey == placeholder` string is the marker.
/// `baseUrl` alone is NEVER considered a marker: a user's own local provider
/// (e.g. `http://localhost:11434/v1` for Ollama) shares the host shape with
/// the proxy and would otherwise be falsely classified as "taken over".
fn entry_has_takeover_markers(entry: &Value, placeholder: &str) -> bool {
    entry
        .as_object()
        .and_then(|obj| obj.get("apiKey"))
        .and_then(|v| v.as_str())
        == Some(placeholder)
}

/// Loopback URL check used ONLY to decide which `baseUrl` to strip from an
/// entry that already passed the marker check (apiKey == placeholder). Never
/// used to classify an entry as taken over (see
/// `entry_has_takeover_markers`).
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

fn providers_have_takeover_markers(providers: &Map<String, Value>, placeholder: &str) -> bool {
    providers
        .values()
        .any(|entry| entry_has_takeover_markers(entry, placeholder))
}

/// Whether a value carries proxy-takeover markers. Accepts either a live
/// snapshot (with `modelsSource`) or a bare provider fragment (DB
/// `settings_config` with top-level `apiKey`/`baseUrl`).
pub fn has_takeover_markers(value: &Value, placeholder: &str) -> bool {
    if let Some(source) = value.get("modelsSource") {
        return source
            .as_str()
            .map(|s| providers_have_takeover_markers(&parse_models_source_providers(s), placeholder))
            .unwrap_or(false);
    }
    entry_has_takeover_markers(value, placeholder)
}

/// The provider key referenced by the `default` role selector, if any.
pub fn get_default_role_provider_id() -> Result<Option<String>, AppError> {
    let roles = get_model_roles()?;
    Ok(roles.get("default").and_then(|v| v.as_str()).and_then(|selector| {
        selector
            .split_once('/')
            .map(|(key, _)| key.trim().to_string())
            .filter(|s| !s.is_empty())
    }))
}

/// The provider entry referenced by the `default` role selector, when both
/// the selector and the models.yml entry exist.
pub fn selected_provider_entry() -> Result<Option<(String, Value)>, AppError> {
    let Some(id) = get_default_role_provider_id()? else {
        return Ok(None);
    };
    let doc = OmpModelsDocument::load()?;
    let entry = doc.providers.get(&id).filter(|e| e.is_object()).cloned();
    Ok(entry.map(|e| (id, e)))
}

/// Apply proxy takeover for `id`: upsert its models.yml entry from
/// `settings_config` with `baseUrl` -> `proxy_base_url` and an inline
/// placeholder `apiKey` marker. `modelRoles` is NOT touched (selectors keep
/// pointing at the same key). Returns the provider key previously referenced
/// by the `default` role so the caller can revert takeover fields on it
/// after a hot switch.
pub fn apply_takeover_and_select(
    id: &str,
    settings_config: &Value,
    proxy_base_url: &str,
    placeholder: &str,
) -> Result<Option<String>, AppError> {
    assert_omp_compatible()?;
    validate_provider_key(id)?;
    let config = parse_provider_config(settings_config.clone())?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Omp config for write: {e}")))?;

    let previous_selection = get_default_role_provider_id()?;

    let mut doc = OmpModelsDocument::load()?;
    let mut entry = provider_to_entry(&config, doc.providers.get(id));
    if let Some(obj) = entry.as_object_mut() {
        obj.insert("baseUrl".to_string(), json!(proxy_base_url));
        obj.insert("apiKey".to_string(), json!(placeholder));
    }
    doc.providers.insert(id.to_string(), entry);
    doc.save()?;

    // Credentials in agent.db stay untouched; the proxy injects the real key.
    Ok(previous_selection)
}

/// Strip takeover markers from an entry map (placeholder apiKey, loopback
/// baseUrl written by us). Returns `None` when the entry carries no markers.
fn strip_takeover_markers(entry: &Value, placeholder: &str) -> Option<Map<String, Value>> {
    if !entry_has_takeover_markers(entry, placeholder) {
        return None;
    }
    let mut stripped = entry.as_object().cloned().unwrap_or_default();
    if stripped.get("apiKey").and_then(|v| v.as_str()) == Some(placeholder) {
        stripped.remove("apiKey");
    }
    // Reached only when apiKey==placeholder, which only happens for entries
    // we wrote via `apply_takeover_and_select` (whose baseUrl pointed at the
    // local proxy). User-provided loopback URLs never carry the marker.
    if stripped
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(is_loopback_url)
        .unwrap_or(false)
    {
        stripped.remove("baseUrl");
    }
    Some(stripped)
}

/// Revert takeover markers on `id`'s entry and rewrite the entry from the DB
/// `settings_config` (restoring its real baseUrl and inline API key).
/// No-op when the entry carries no markers.
pub fn revert_provider_takeover(
    id: &str,
    settings_config: &Value,
    placeholder: &str,
) -> Result<(), AppError> {
    assert_omp_compatible()?;
    validate_provider_key(id)?;
    let config = parse_provider_config(settings_config.clone())?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Omp config for write: {e}")))?;

    let mut doc = OmpModelsDocument::load()?;
    let Some(existing) = doc.providers.get(id) else {
        return Ok(());
    };
    let Some(stripped) = strip_takeover_markers(existing, placeholder) else {
        return Ok(());
    };

    let entry = provider_to_entry(&config, Some(&Value::Object(stripped)));
    doc.providers.insert(id.to_string(), entry);
    doc.save()
}

/// Remove takeover markers from a single provider entry without a DB config
/// to restore from (provider not managed by CC Switch). Leaves the entry
/// otherwise intact; a removed local-proxy baseUrl is simply dropped.
pub fn remove_provider_takeover_markers(id: &str, placeholder: &str) -> Result<(), AppError> {
    assert_omp_compatible()?;

    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Omp config for write: {e}")))?;

    let mut doc = OmpModelsDocument::load()?;
    let Some(existing) = doc.providers.get(id) else {
        return Ok(());
    };
    let Some(stripped) = strip_takeover_markers(existing, placeholder) else {
        return Ok(());
    };
    doc.providers.insert(id.to_string(), Value::Object(stripped));
    doc.save()
}

/// Last-resort cleanup: strip takeover markers from EVERY provider entry in
/// models.yml. Returns whether anything changed. Caller must hold
/// [`write_lock`].
fn remove_takeover_markers_all_locked(placeholder: &str) -> Result<bool, AppError> {
    let mut doc = OmpModelsDocument::load()?;
    let mut changed = false;
    let ids: Vec<String> = doc.providers.keys().cloned().collect();
    for id in ids {
        let existing = doc.providers.get(&id).expect("id collected above").clone();
        if let Some(stripped) = strip_takeover_markers(&existing, placeholder) {
            doc.providers.insert(id, Value::Object(stripped));
            changed = true;
        }
    }
    if !changed {
        return Ok(false);
    }
    doc.save()?;
    Ok(true)
}

/// Last-resort cleanup: strip takeover markers from EVERY provider entry in
/// models.yml (backup-missing restore fallback).
pub fn remove_takeover_markers_all(placeholder: &str) -> Result<bool, AppError> {
    let _guard = write_lock()
        .lock()
        .map_err(|e| AppError::Message(format!("Failed to lock Omp config for write: {e}")))?;
    remove_takeover_markers_all_locked(placeholder)
}

/// Patch a backup snapshot with a provider's pristine (non-takeover) entry:
/// replaces `providers.<id>` inside `modelsSource` (section surgery, so
/// everything outside the section survives). Used to keep the restore backup
/// aligned with provider edits during takeover. `modelRoles` is untouched.
pub fn patch_snapshot_provider(
    snapshot: &mut Value,
    id: &str,
    settings_config: &Value,
) -> Result<(), AppError> {
    validate_provider_key(id)?;
    let config = parse_provider_config(settings_config.clone())?;

    let source = snapshot
        .get("modelsSource")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut providers = parse_models_source_providers(&source);
    let entry = provider_to_entry(&config, providers.get(id));
    providers.insert(id.to_string(), entry);

    snapshot["modelsSource"] = json!(splice_providers_section(&source, &providers)?);
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home<F: FnOnce(PathBuf)>(f: F) {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("omp-agent");
        fs::create_dir_all(&home).unwrap();
        std::env::set_var("PI_CODING_AGENT_DIR", &home);
        f(home);
        std::env::remove_var("PI_CODING_AGENT_DIR");
    }

    fn sample_config() -> OmpProviderConfig {
        OmpProviderConfig {
            api: "openai-completions".into(),
            api_key: Some("sk-test".into()),
            base_url: Some("http://localhost:11434/v1".into()),
            models: vec![OmpModel {
                id: "minimax-m3".into(),
                name: Some("MiniMax M3".into()),
                reasoning: Some(false),
                context_window: Some(100_000),
                max_tokens: Some(32_000),
                ..Default::default()
            }],
            default_model_id: Some("minimax-m3".into()),
            ..Default::default()
        }
    }

    #[test]
    fn version_parse() {
        assert_eq!(parse_version("17.2.2"), Some((17, 2, 2)));
        assert_eq!(parse_version("v17.0.0"), Some((17, 0, 0)));
        assert_eq!(parse_version("17.0.0-beta.3"), Some((17, 0, 0)));
        assert_eq!(parse_version("garbage"), None);
        assert_eq!(extract_omp_version("omp/17.2.2"), Some("17.2.2".into()));
        assert_eq!(extract_omp_version("omp/17.2.2\n"), Some("17.2.2".into()));
        assert_eq!(extract_omp_version("17.2.2"), Some("17.2.2".into()));
        assert_eq!(extract_omp_version("garbage"), None);
    }

    #[test]
    fn provider_key_validation() {
        assert!(validate_provider_key("spark").is_ok());
        assert!(validate_provider_key("a.b_c-d9").is_ok());
        assert!(validate_provider_key("0").is_ok());
        assert!(validate_provider_key("").is_err());
        assert!(validate_provider_key("Spark").is_err());
        assert!(validate_provider_key("a/b").is_err());
        assert!(validate_provider_key("a:b").is_err());
        assert!(validate_provider_key("-a").is_err());
        assert!(validate_provider_key(".a").is_err());
        assert!(validate_provider_key("managed:anthropic").is_err());
    }

    #[test]
    fn role_validation() {
        for role in OMP_ROLES {
            assert!(validate_role(role).is_ok(), "role {role} should be valid");
        }
        assert!(validate_role("bogus").is_err());
        assert!(validate_role("").is_err());
        assert!(validate_role("Default").is_err());
    }

    #[test]
    #[serial]
    fn set_and_get_provider_roundtrip() {
        with_temp_home(|_home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("local-ollama", value, "default").unwrap();

            let providers = get_providers().unwrap();
            let entry = providers.get("local-ollama").unwrap();
            assert_eq!(entry["api"], "openai-completions");
            assert_eq!(entry["baseUrl"], "http://localhost:11434/v1");
            assert_eq!(entry["apiKey"], "sk-test");
            assert_eq!(entry["models"][0]["id"], "minimax-m3");
            assert_eq!(entry["models"][0]["contextWindow"], 100_000);
            assert_eq!(entry["_ccSource"], "managed");

            // Role selector: `<key>/<defaultModelId>`.
            let roles = get_model_roles().unwrap();
            assert_eq!(
                roles.get("default").and_then(|v| v.as_str()),
                Some("local-ollama/minimax-m3")
            );
        });
    }

    #[test]
    #[serial]
    fn legacy_type_alias_is_accepted_on_read() {
        let config: OmpProviderConfig = serde_json::from_value(json!({
            "type": "anthropic-messages",
            "baseUrl": "https://example.com",
        }))
        .unwrap();
        assert_eq!(config.api, "anthropic-messages");
    }

    #[test]
    #[serial]
    fn section_splice_preserves_outside_bytes_and_comments() {
        with_temp_home(|home| {
            let original = "# top comment\nproviders:\n  hand-written:\n    baseUrl: https://example.com\n    apiKey: $HAND_KEY\notherTopLevel: true  # trailing comment\n";
            fs::write(home.join("models.yml"), original).unwrap();

            let value = serde_json::to_value(sample_config()).unwrap();
            set_provider("local-ollama", value).unwrap();

            let text = fs::read_to_string(home.join("models.yml")).unwrap();
            assert!(
                text.starts_with("# top comment\nproviders:\n"),
                "bytes before the section body must be preserved: {text}"
            );
            assert!(
                text.ends_with("otherTopLevel: true  # trailing comment\n"),
                "bytes after the section must be preserved: {text}"
            );
            assert!(text.contains("hand-written"), "user entry kept: {text}");
            assert!(text.contains("$HAND_KEY"), "user entry fields kept: {text}");
            assert!(text.contains("local-ollama"), "new entry inserted: {text}");
            assert!(
                text.contains("_ccSource: managed"),
                "managed marker stamped: {text}"
            );
            // The hand-written entry carries NO managed marker.
            let providers = parse_providers_section(&text).unwrap();
            assert!(!is_managed_provider(&providers["hand-written"]));
            assert!(is_managed_provider(&providers["local-ollama"]));
        });
    }

    #[test]
    #[serial]
    fn managed_entry_upsert_preserves_unknown_fields() {
        with_temp_home(|home| {
            fs::write(
                home.join("models.yml"),
                "providers:\n  local-ollama:\n    baseUrl: http://old:1/v1\n    customField: keep-me\n    _ccSource: managed\n",
            )
            .unwrap();

            let value = serde_json::to_value(sample_config()).unwrap();
            set_provider("local-ollama", value).unwrap();

            let text = fs::read_to_string(home.join("models.yml")).unwrap();
            let providers = parse_providers_section(&text).unwrap();
            let entry = &providers["local-ollama"];
            assert_eq!(entry["baseUrl"], "http://localhost:11434/v1");
            assert_eq!(entry["customField"], "keep-me", "unowned key preserved");
            assert_eq!(entry["_ccSource"], "managed");
        });
    }

    #[test]
    #[serial]
    fn remove_provider_keeps_user_entries_and_strips_roles() {
        with_temp_home(|home| {
            fs::write(
                home.join("models.yml"),
                "providers:\n  hand-written:\n    baseUrl: https://example.com\n",
            )
            .unwrap();
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("tmp", value.clone(), "default").unwrap();
            set_provider("second", value).unwrap();
            set_model_role("plan", "second").unwrap();

            let roles = get_model_roles().unwrap();
            assert_eq!(
                roles.get("plan").and_then(|v| v.as_str()),
                Some("second/minimax-m3")
            );

            remove_provider("second").unwrap();
            let providers = get_providers().unwrap();
            assert!(providers.get("second").is_none());
            assert!(
                providers.get("hand-written").is_some(),
                "user entry survives removal of another provider"
            );
            let roles = get_model_roles().unwrap();
            assert!(roles.get("plan").is_none(), "stale role stripped");
            assert_eq!(
                roles.get("default").and_then(|v| v.as_str()),
                Some("tmp/minimax-m3")
            );

            remove_provider("tmp").unwrap();
            let roles = get_model_roles().unwrap();
            assert!(roles.is_empty(), "all roles stripped");
            // modelRoles section removed entirely; file stays valid YAML.
            let config_text = fs::read_to_string(home.join("config.yml")).unwrap();
            let parsed: serde_yaml::Value = serde_yaml::from_str(&config_text).unwrap();
            assert!(parsed.get("modelRoles").is_none());
        });
    }

    #[test]
    #[serial]
    fn model_roles_set_clear_and_other_keys_untouched() {
        with_temp_home(|home| {
            fs::write(
                home.join("config.yml"),
                "theme: dark\nmodelRoles:\n  default: other/model-x\nwebSearchOrder:\n  - google\n",
            )
            .unwrap();
            let value = serde_json::to_value(sample_config()).unwrap();
            set_provider("local-ollama", value).unwrap();

            set_model_role("default", "local-ollama").unwrap();
            let roles = get_model_roles().unwrap();
            assert_eq!(
                roles.get("default").and_then(|v| v.as_str()),
                Some("local-ollama/minimax-m3")
            );

            let text = fs::read_to_string(home.join("config.yml")).unwrap();
            assert!(text.contains("theme: dark"), "other keys untouched: {text}");
            assert!(
                text.contains("webSearchOrder:\n  - google\n"),
                "trailing keys untouched: {text}"
            );

            clear_model_role("default").unwrap();
            let roles = get_model_roles().unwrap();
            assert!(roles.is_empty());
            let text = fs::read_to_string(home.join("config.yml")).unwrap();
            assert!(text.contains("theme: dark"), "other keys still there: {text}");
            assert!(!text.contains("modelRoles"), "empty section removed: {text}");

            // Clearing an absent role is a no-op success.
            clear_model_role("smol").unwrap();
            // Invalid roles are rejected before any write.
            assert!(set_model_role("bogus", "local-ollama").is_err());
            assert!(clear_model_role("bogus").is_err());
            // Unknown provider cannot be assigned.
            assert!(set_model_role("default", "missing").is_err());
        });
    }

    #[test]
    #[serial]
    fn invalid_provider_key_is_rejected() {
        with_temp_home(|_home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            assert!(set_provider("managed:anthropic", value.clone()).is_err());
            assert!(set_provider("a/b", value.clone()).is_err());
            assert!(remove_provider("a/b").is_err());
        });
    }

    #[test]
    #[serial]
    fn models_yml_parse_failure_surfaces_error() {
        with_temp_home(|home| {
            fs::write(home.join("models.yml"), "providers: [unclosed\n").unwrap();
            let value = serde_json::to_value(sample_config()).unwrap();
            assert!(set_provider("local-ollama", value).is_err());
            assert!(get_providers().is_err());
        });
    }

    #[test]
    #[serial]
    fn optimistic_concurrency_conflict_is_reported() {
        with_temp_home(|home| {
            fs::write(home.join("models.yml"), "providers: {}\n").unwrap();
            let mut doc = OmpModelsDocument::load().unwrap();
            // Concurrent external edit after load.
            fs::write(home.join("models.yml"), "providers:\n  other: {}\n").unwrap();
            doc.providers.insert("x".to_string(), json!({}));
            let err = doc.save().expect_err("must refuse to clobber");
            assert!(
                err.to_string().contains("changed on disk") || err.to_string().contains("已被修改"),
                "unexpected error: {err}"
            );
        });
    }

    #[test]
    #[serial]
    fn oauth_detection_with_missing_or_present_db() {
        with_temp_home(|home| {
            // Missing agent.db -> false.
            assert!(!provider_has_oauth_credential("anthropic"));

            // Seed a real SQLite db with the auth_credentials table.
            let conn = rusqlite::Connection::open(home.join("agent.db")).unwrap();
            conn.execute(
                "CREATE TABLE auth_credentials (provider TEXT, credential_type TEXT, data TEXT)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO auth_credentials VALUES ('anthropic', 'oauth', '{}')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO auth_credentials VALUES('plain', 'api_key', '{}')",
                [],
            )
            .unwrap();
            drop(conn);

            assert!(provider_has_oauth_credential("anthropic"));
            assert!(!provider_has_oauth_credential("plain"));
            assert!(!provider_has_oauth_credential("missing"));
        });
    }

    #[test]
    #[serial]
    fn takeover_snapshot_roundtrip_restores_verbatim() {
        with_temp_home(|home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("local-ollama", value.clone(), "default").unwrap();
            set_provider("second", value).unwrap();

            let before = fs::read_to_string(home.join("models.yml")).unwrap();
            let snapshot = read_live_snapshot().unwrap();
            assert_eq!(snapshot["modelsSource"].as_str().unwrap(), before);

            let previous = apply_takeover_and_select(
                "local-ollama",
                &serde_json::to_value(sample_config()).unwrap(),
                "http://127.0.0.1:15721/omp",
                "PROXY_MANAGED",
            )
            .unwrap();
            assert_eq!(previous.as_deref(), Some("local-ollama"));

            let text = fs::read_to_string(home.join("models.yml")).unwrap();
            let providers = parse_providers_section(&text).unwrap();
            let entry = &providers["local-ollama"];
            assert_eq!(entry["baseUrl"], "http://127.0.0.1:15721/omp");
            assert_eq!(entry["apiKey"], "PROXY_MANAGED");

            // modelRoles untouched by takeover.
            let roles = get_model_roles().unwrap();
            assert_eq!(
                roles.get("default").and_then(|v| v.as_str()),
                Some("local-ollama/minimax-m3")
            );

            // Pre-takeover snapshot has no markers; post-takeover does.
            assert!(!has_takeover_markers(&snapshot, "PROXY_MANAGED"));
            let taken_over = read_live_snapshot().unwrap();
            assert!(has_takeover_markers(&taken_over, "PROXY_MANAGED"));

            write_live_snapshot(&snapshot, "PROXY_MANAGED").unwrap();
            let after = fs::read_to_string(home.join("models.yml")).unwrap();
            assert_eq!(after, before, "restore must be verbatim");
        });
    }

    #[test]
    #[serial]
    fn takeover_revert_restores_db_config() {
        with_temp_home(|home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("local-ollama", value.clone(), "default").unwrap();

            apply_takeover_and_select(
                "local-ollama",
                &value,
                "http://127.0.0.1:15721/omp",
                "PROXY_MANAGED",
            )
            .unwrap();

            revert_provider_takeover("local-ollama", &value, "PROXY_MANAGED").unwrap();
            let text = fs::read_to_string(home.join("models.yml")).unwrap();
            let providers = parse_providers_section(&text).unwrap();
            let entry = &providers["local-ollama"];
            assert_eq!(entry["baseUrl"], "http://localhost:11434/v1");
            assert_eq!(entry["apiKey"], "sk-test");
            assert_eq!(entry["_ccSource"], "managed");

            // Revert without markers is a no-op.
            revert_provider_takeover("local-ollama", &value, "PROXY_MANAGED").unwrap();
        });
    }

    #[test]
    #[serial]
    fn loopback_baseurl_alone_is_not_a_takeover_marker() {
        with_temp_home(|_home| {
            // User-authored loopback provider (Ollama-shaped) without the
            // placeholder apiKey must never be classified as taken over.
            upsert_and_select(
                "user-ollama",
                serde_json::to_value(sample_config()).unwrap(),
                "default",
            )
            .unwrap();

            let snapshot = read_live_snapshot().unwrap();
            assert!(
                !has_takeover_markers(&snapshot, "PROXY_MANAGED"),
                "user's loopback baseUrl must not be misread as a takeover marker"
            );

            let changed = remove_takeover_markers_all("PROXY_MANAGED").unwrap();
            assert!(!changed, "no entries should be touched");

            let providers = get_providers().unwrap();
            assert_eq!(
                providers["user-ollama"]["baseUrl"],
                "http://localhost:11434/v1"
            );
            assert_eq!(providers["user-ollama"]["apiKey"], "sk-test");
        });
    }

    #[test]
    #[serial]
    fn remove_takeover_markers_all_strips_every_marked_entry() {
        with_temp_home(|_home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("local-ollama", value.clone(), "default").unwrap();
            set_provider("second", value.clone()).unwrap();
            apply_takeover_and_select(
                "local-ollama",
                &value,
                "http://127.0.0.1:15721/omp",
                "PROXY_MANAGED",
            )
            .unwrap();
            apply_takeover_and_select(
                "second",
                &value,
                "http://127.0.0.1:15721/omp",
                "PROXY_MANAGED",
            )
            .unwrap();

            let changed = remove_takeover_markers_all("PROXY_MANAGED").unwrap();
            assert!(changed);
            let snapshot = read_live_snapshot().unwrap();
            assert!(!has_takeover_markers(&snapshot, "PROXY_MANAGED"));
            let providers = get_providers().unwrap();
            assert!(providers.get("local-ollama").is_some());
            assert!(providers.get("second").is_some());
        });
    }

    #[test]
    #[serial]
    fn patch_snapshot_provider_updates_entry() {
        with_temp_home(|_home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("local-ollama", value.clone(), "default").unwrap();
            set_provider("second", value).unwrap();

            let mut snapshot = read_live_snapshot().unwrap();
            let mut edited = sample_config();
            edited.base_url = Some("https://edited.example.com/v1".into());
            edited.api_key = Some("sk-edited".into());
            let edited_value = serde_json::to_value(&edited).unwrap();
            patch_snapshot_provider(&mut snapshot, "second", &edited_value).unwrap();

            let providers =
                parse_models_source_providers(snapshot["modelsSource"].as_str().unwrap());
            assert_eq!(
                providers["second"]["baseUrl"],
                "https://edited.example.com/v1"
            );
            assert_eq!(providers["second"]["apiKey"], "sk-edited");
            assert!(providers.get("local-ollama").is_some());
        });
    }

    #[test]
    #[serial]
    fn read_live_settings_shape() {
        with_temp_home(|_home| {
            let value = serde_json::to_value(sample_config()).unwrap();
            upsert_and_select("local-ollama", value, "plan").unwrap();

            let live = read_live_settings().unwrap();
            assert_eq!(
                live["config"]["modelRoles"]["plan"],
                "local-ollama/minimax-m3"
            );
            assert_eq!(
                live["models"]["local-ollama"]["baseUrl"],
                "http://localhost:11434/v1"
            );
        });
    }
}
