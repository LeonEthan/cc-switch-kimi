//! Kimi Code MCP sync and import.
//!
//! Kimi Code stores MCP servers in `~/.kimi-code/mcp.json`:
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "filesystem": {
//!       "command": "npx",
//!       "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
//!     },
//!     "linear": {
//!       "url": "https://mcp.linear.app/mcp"
//!     }
//!   }
//! }
//! ```
//!
//! Format is close to Claude: stdio has `command`/`args`/`env` (no required `type`),
//! HTTP has `url` (optional `transport: "sse"` for legacy SSE).

use serde_json::{json, Map, Value};

use crate::app_config::{McpApps, McpServer, MultiAppConfig};
use crate::error::AppError;
use crate::kimi_code_config;

use super::validation::validate_server_spec;

fn should_sync() -> bool {
    kimi_code_config::get_kimi_code_dir().exists()
}

/// Convert CC Switch unified spec → Kimi Code mcp.json entry.
fn convert_to_kimicode(spec: &Value) -> Result<Value, AppError> {
    let obj = spec
        .as_object()
        .ok_or_else(|| AppError::McpValidation("MCP spec must be a JSON object".into()))?;

    let typ = obj.get("type").and_then(|v| v.as_str()).unwrap_or("stdio");
    let mut result = Map::new();

    match typ {
        "stdio" => {
            if let Some(cmd) = obj.get("command") {
                result.insert("command".into(), cmd.clone());
            }
            if let Some(args) = obj.get("args") {
                result.insert("args".into(), args.clone());
            }
            if let Some(env) = obj.get("env") {
                if env.is_object() && !env.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                    result.insert("env".into(), env.clone());
                }
            }
            if let Some(cwd) = obj.get("cwd") {
                result.insert("cwd".into(), cwd.clone());
            }
        }
        "http" => {
            if let Some(url) = obj.get("url") {
                result.insert("url".into(), url.clone());
            }
            if let Some(headers) = obj.get("headers") {
                if headers.is_object() && !headers.as_object().map(|o| o.is_empty()).unwrap_or(true)
                {
                    result.insert("headers".into(), headers.clone());
                }
            }
        }
        "sse" => {
            if let Some(url) = obj.get("url") {
                result.insert("url".into(), url.clone());
            }
            result.insert("transport".into(), json!("sse"));
            if let Some(headers) = obj.get("headers") {
                if headers.is_object() && !headers.as_object().map(|o| o.is_empty()).unwrap_or(true)
                {
                    result.insert("headers".into(), headers.clone());
                }
            }
        }
        other => {
            return Err(AppError::McpValidation(format!(
                "Unknown MCP type for Kimi Code: {other}"
            )));
        }
    }

    // Preserve optional timeout fields when present
    for key in [
        "enabled",
        "startupTimeoutMs",
        "toolTimeoutMs",
        "enabledTools",
        "disabledTools",
        "bearerTokenEnvVar",
    ] {
        if let Some(v) = obj.get(key) {
            result.insert(key.to_string(), v.clone());
        }
    }

    Ok(Value::Object(result))
}

/// Convert Kimi Code mcp.json entry → CC Switch unified spec.
fn convert_from_kimicode(spec: &Value) -> Result<Value, AppError> {
    let obj = spec.as_object().ok_or_else(|| {
        AppError::McpValidation("Kimi Code MCP spec must be a JSON object".into())
    })?;

    let mut result = Map::new();

    if obj.contains_key("command") {
        result.insert("type".into(), json!("stdio"));
        if let Some(cmd) = obj.get("command") {
            result.insert("command".into(), cmd.clone());
        }
        if let Some(args) = obj.get("args") {
            result.insert("args".into(), args.clone());
        }
        if let Some(env) = obj.get("env") {
            result.insert("env".into(), env.clone());
        }
        if let Some(cwd) = obj.get("cwd") {
            result.insert("cwd".into(), cwd.clone());
        }
    } else if obj.contains_key("url") {
        let transport = obj
            .get("transport")
            .and_then(|v| v.as_str())
            .unwrap_or("http");
        result.insert(
            "type".into(),
            json!(if transport == "sse" { "sse" } else { "http" }),
        );
        if let Some(url) = obj.get("url") {
            result.insert("url".into(), url.clone());
        }
        if let Some(headers) = obj.get("headers") {
            result.insert("headers".into(), headers.clone());
        }
    } else {
        return Err(AppError::McpValidation(
            "Kimi Code MCP entry must have 'command' or 'url'".into(),
        ));
    }

    Ok(Value::Object(result))
}

pub fn sync_single_server_to_kimicode(
    _config: &MultiAppConfig,
    id: &str,
    server_spec: &Value,
) -> Result<(), AppError> {
    if !should_sync() {
        return Ok(());
    }
    validate_server_spec(server_spec)?;
    let converted = convert_to_kimicode(server_spec)?;
    kimi_code_config::set_mcp_server(id, converted)
}

pub fn remove_server_from_kimicode(id: &str) -> Result<(), AppError> {
    if !should_sync() {
        return Ok(());
    }
    kimi_code_config::remove_mcp_server(id)
}

pub fn import_from_kimicode(config: &mut MultiAppConfig) -> Result<usize, AppError> {
    let servers = kimi_code_config::get_mcp_servers()?;
    if servers.is_empty() {
        return Ok(0);
    }

    let mut imported = 0;
    let store = config.mcp.servers.get_or_insert_with(Default::default);

    for (id, spec) in servers {
        let unified = match convert_from_kimicode(&spec) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("Skip Kimi Code MCP '{id}': {e}");
                continue;
            }
        };

        if let Some(existing) = store.get_mut(&id) {
            existing.apps.kimicode = true;
            continue;
        }

        store.insert(
            id.clone(),
            McpServer {
                id: id.clone(),
                name: id.clone(),
                server: unified,
                apps: McpApps {
                    kimicode: true,
                    ..Default::default()
                },
                description: None,
                homepage: None,
                docs: None,
                tags: Vec::new(),
            },
        );
        imported += 1;
    }

    Ok(imported)
}
