//! omp (`omp` CLI) Provider Adapter
//!
//! omp providers are per-provider protocol-typed via `settings_config.api`
//! (omp's KnownApi: `anthropic-messages`, `openai-completions`, ...; `type` is
//! accepted as a read alias). The /omp proxy namespace only forwards
//! same-protocol traffic — no Anthropic ↔ OpenAI conversion — so this adapter
//! never transforms bodies; it only resolves the upstream base URL,
//! credentials, and auth header style per provider:
//!
//! - `anthropic-messages` (default): `x-api-key`, or `Authorization: Bearer`
//!   when the provider sets `authHeader: true` (models.yml semantics).
//! - `openai-completions` / `openai-responses`: `Authorization: Bearer`.
//!
//! `apiKey` values that are a single env-var reference (`$VAR` / `${VAR}`,
//! models.yml template syntax) are resolved from the process environment at
//! request time; anything else is sent as a literal.

use super::adapter::auth_header_value;
use super::{AuthInfo, AuthStrategy, ProviderAdapter};
use crate::provider::Provider;
use crate::proxy::error::ProxyError;

/// omp adapter
pub struct OmpAdapter;

/// Protocol family spoken by an omp provider (from `settings_config.api`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmpProtocol {
    AnthropicMessages,
    OpenAiCompletions,
    OpenAiResponses,
    GoogleGenerativeAi,
    Other,
}

fn read_api(value: &serde_json::Value) -> Option<&str> {
    value
        .get("api")
        .or_else(|| value.get("type"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn protocol_of(api: &str) -> OmpProtocol {
    match api {
        "anthropic-messages" => OmpProtocol::AnthropicMessages,
        "openai-completions" => OmpProtocol::OpenAiCompletions,
        "openai-responses" => OmpProtocol::OpenAiResponses,
        "google-generative-ai" => OmpProtocol::GoogleGenerativeAi,
        _ => OmpProtocol::Other,
    }
}

/// The provider's declared wire protocol. omp's KnownApi default is
/// `anthropic-messages` when `api` is absent (mirrors omp's own fallback).
pub fn omp_provider_protocol(provider: &Provider) -> OmpProtocol {
    read_api(&provider.settings_config)
        .map(protocol_of)
        .unwrap_or(OmpProtocol::AnthropicMessages)
}

/// Whether the provider can serve a client request that arrived on
/// `/omp/v1/messages` (client speaks Anthropic Messages). A model-level `api`
/// override in the provider's model list also qualifies.
pub fn omp_provider_supports_anthropic_messages(provider: &Provider) -> bool {
    omp_provider_supports(provider, OmpProtocol::AnthropicMessages)
}

/// Whether the provider can serve `/omp/v1/chat/completions` (client speaks
/// OpenAI Chat Completions).
pub fn omp_provider_supports_chat_completions(provider: &Provider) -> bool {
    omp_provider_supports(provider, OmpProtocol::OpenAiCompletions)
}

fn omp_provider_supports(provider: &Provider, want: OmpProtocol) -> bool {
    if omp_provider_protocol(provider) == want {
        return true;
    }
    // Model-level api override: any model in the provider's catalog that
    // speaks the wanted protocol makes the provider eligible for the route.
    provider
        .settings_config
        .get("models")
        .and_then(|m| m.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(read_api)
                .any(|api| protocol_of(api) == want)
        })
        .unwrap_or(false)
}

/// Resolve a models.yml-style config value: a whole-string `$VAR`/`${VAR}`
/// reference reads the process environment; anything else is a literal.
/// (`!command` templates are intentionally not executed by the proxy.)
fn resolve_env_template(raw: &str) -> String {
    let trimmed = raw.trim();
    let name = trimmed
        .strip_prefix('$')
        .map(|rest| {
            rest.strip_prefix('{')
                .and_then(|inner| inner.strip_suffix('}'))
                .unwrap_or(rest)
        })
        .filter(|name| {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
        });
    match name {
        Some(var) => std::env::var(var).unwrap_or_else(|_| raw.to_string()),
        None => raw.to_string(),
    }
}

impl OmpAdapter {
    pub fn new() -> Self {
        Self
    }

    fn extract_key(&self, provider: &Provider) -> Option<String> {
        provider
            .settings_config
            .get("apiKey")
            .or_else(|| provider.settings_config.get("api_key"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(resolve_env_template)
    }
}

impl ProviderAdapter for OmpAdapter {
    fn name(&self) -> &'static str {
        "Omp"
    }

    fn extract_base_url(&self, provider: &Provider) -> Result<String, ProxyError> {
        if let Some(url) = provider
            .settings_config
            .get("baseUrl")
            .or_else(|| provider.settings_config.get("base_url"))
            .or_else(|| provider.settings_config.get("baseURL"))
            .and_then(|v| v.as_str())
        {
            let trimmed = url.trim().trim_end_matches('/');
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        Err(ProxyError::ConfigError(
            "Omp Provider 缺少 baseUrl 配置".to_string(),
        ))
    }

    fn extract_auth(&self, provider: &Provider) -> Option<AuthInfo> {
        let key = self.extract_key(provider)?;
        let strategy = match omp_provider_protocol(provider) {
            OmpProtocol::AnthropicMessages => {
                // models.yml `authHeader: true` → Authorization: Bearer;
                // otherwise the Anthropic SDK default (x-api-key).
                let use_bearer = provider
                    .settings_config
                    .get("authHeader")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if use_bearer {
                    AuthStrategy::Bearer
                } else {
                    AuthStrategy::Anthropic
                }
            }
            _ => AuthStrategy::Bearer,
        };
        Some(AuthInfo::new(key, strategy))
    }

    fn build_url(&self, base_url: &str, endpoint: &str) -> String {
        let base = base_url.trim_end_matches('/');
        let ep = endpoint.trim_start_matches('/');

        // Endpoint already carries its version prefix (/v1/messages):
        // plain concat + dedupe (Claude-style).
        if ep.starts_with("v1/") || ep.starts_with("v1beta/") {
            let mut url = format!("{base}/{ep}");
            while url.contains("/v1/v1") {
                url = url.replace("/v1/v1", "/v1");
            }
            return url;
        }

        // Version-less endpoint (/chat/completions): mirror Codex rules —
        // origin-only bases get /v1, custom prefixes pass through.
        if base.ends_with("/v1") {
            format!("{base}/{ep}")
        } else if super::codex::is_origin_only_url(base) {
            format!("{base}/v1/{ep}")
        } else {
            format!("{base}/{ep}")
        }
    }

    fn get_auth_headers(
        &self,
        auth: &AuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        use http::HeaderName;
        // anthropic-version is forwarded from the omp client by the forwarder.
        Ok(match auth.strategy {
            AuthStrategy::Anthropic => vec![(
                HeaderName::from_static("x-api-key"),
                auth_header_value(&auth.api_key)?,
            )],
            _ => vec![(
                HeaderName::from_static("authorization"),
                auth_header_value(&format!("Bearer {}", auth.api_key))?,
            )],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test".to_string(),
            settings_config: config,
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn protocol_defaults_to_anthropic_messages() {
        let p = make_provider(json!({"baseUrl": "https://api.anthropic.com"}));
        assert_eq!(omp_provider_protocol(&p), OmpProtocol::AnthropicMessages);
        assert!(omp_provider_supports_anthropic_messages(&p));
        assert!(!omp_provider_supports_chat_completions(&p));
    }

    #[test]
    fn openai_completions_provider_supports_chat_route_only() {
        let p = make_provider(json!({
            "api": "openai-completions",
            "baseUrl": "https://api.openai.com/v1",
            "apiKey": "sk-test"
        }));
        assert_eq!(omp_provider_protocol(&p), OmpProtocol::OpenAiCompletions);
        assert!(omp_provider_supports_chat_completions(&p));
        assert!(!omp_provider_supports_anthropic_messages(&p));
    }

    #[test]
    fn type_field_is_accepted_as_read_alias() {
        let p = make_provider(json!({
            "type": "openai-completions",
            "baseUrl": "https://api.openai.com/v1"
        }));
        assert_eq!(omp_provider_protocol(&p), OmpProtocol::OpenAiCompletions);
    }

    #[test]
    fn model_level_api_override_qualifies_provider() {
        let p = make_provider(json!({
            "api": "anthropic-messages",
            "models": [{"id": "m1", "api": "openai-completions"}]
        }));
        assert!(omp_provider_supports_anthropic_messages(&p));
        assert!(omp_provider_supports_chat_completions(&p));
    }

    #[test]
    fn anthropic_auth_uses_x_api_key_by_default_and_bearer_with_auth_header() {
        let adapter = OmpAdapter::new();
        let p = make_provider(json!({"apiKey": "sk-ant"}));
        let auth = adapter.extract_auth(&p).unwrap();
        assert_eq!(auth.strategy, AuthStrategy::Anthropic);
        let headers = adapter.get_auth_headers(&auth).unwrap();
        assert_eq!(headers[0].0.as_str(), "x-api-key");
        assert_eq!(headers[0].1.to_str().unwrap(), "sk-ant");

        let p_bearer = make_provider(json!({"apiKey": "sk-ant", "authHeader": true}));
        let auth_bearer = adapter.extract_auth(&p_bearer).unwrap();
        assert_eq!(auth_bearer.strategy, AuthStrategy::Bearer);
        let headers = adapter.get_auth_headers(&auth_bearer).unwrap();
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(headers[0].1.to_str().unwrap(), "Bearer sk-ant");
    }

    #[test]
    fn openai_auth_is_bearer() {
        let adapter = OmpAdapter::new();
        let p = make_provider(json!({"api": "openai-completions", "apiKey": "sk-oai"}));
        let auth = adapter.extract_auth(&p).unwrap();
        assert_eq!(auth.strategy, AuthStrategy::Bearer);
    }

    #[test]
    fn env_template_key_resolves_from_process_env() {
        std::env::set_var("OMP_ADAPTER_TEST_KEY", "resolved-key");
        assert_eq!(
            resolve_env_template("$OMP_ADAPTER_TEST_KEY"),
            "resolved-key"
        );
        assert_eq!(
            resolve_env_template("${OMP_ADAPTER_TEST_KEY}"),
            "resolved-key"
        );
        // Unknown var falls back to the literal so the failure is visible upstream.
        assert_eq!(
            resolve_env_template("$OMP_ADAPTER_TEST_MISSING"),
            "$OMP_ADAPTER_TEST_MISSING"
        );
        // Non-template values pass through untouched.
        assert_eq!(resolve_env_template("sk-literal"), "sk-literal");
        // Mixed templates are not resolved (left literal).
        assert_eq!(
            resolve_env_template("prefix-$OMP_ADAPTER_TEST_KEY"),
            "prefix-$OMP_ADAPTER_TEST_KEY"
        );
        std::env::remove_var("OMP_ADAPTER_TEST_KEY");
    }

    #[test]
    fn build_url_handles_versioned_and_versionless_endpoints() {
        let adapter = OmpAdapter::new();
        // Anthropic-style: endpoint carries /v1
        assert_eq!(
            adapter.build_url("https://api.anthropic.com", "/v1/messages"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            adapter.build_url("https://api.kimi.com/coding", "/v1/messages"),
            "https://api.kimi.com/coding/v1/messages"
        );
        // Base already ends with /v1 + versioned endpoint → dedupe
        assert_eq!(
            adapter.build_url("https://api.anthropic.com/v1", "/v1/messages"),
            "https://api.anthropic.com/v1/messages"
        );
        // OpenAI-style: version-less endpoint
        assert_eq!(
            adapter.build_url("https://api.openai.com/v1", "/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            adapter.build_url("https://api.openai.com", "/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            adapter.build_url("https://gateway.example.com/openai", "/chat/completions"),
            "https://gateway.example.com/openai/chat/completions"
        );
    }

    #[test]
    fn extract_base_url_reads_camel_or_snake_case() {
        let adapter = OmpAdapter::new();
        let p = make_provider(json!({"baseUrl": "https://a.example.com/"}));
        assert_eq!(
            adapter.extract_base_url(&p).unwrap(),
            "https://a.example.com"
        );
        let p2 = make_provider(json!({"base_url": "https://b.example.com"}));
        assert_eq!(
            adapter.extract_base_url(&p2).unwrap(),
            "https://b.example.com"
        );
        let p3 = make_provider(json!({}));
        assert!(adapter.extract_base_url(&p3).is_err());
    }
}
