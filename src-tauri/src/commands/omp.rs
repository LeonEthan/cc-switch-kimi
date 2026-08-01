use serde_json::{Map, Value};
use tauri::State;

use crate::app_config::AppType;
use crate::error::AppError;
use crate::omp_config;
use crate::store::AppState;

/// Import providers from Omp live config (`models.yml`) into the database.
///
/// Tauri 2 binds command names on the JS side; `rename_all = "camelCase"` makes
/// this `importOmpProvidersFromLive` in JavaScript — matching `CONTRIBUTING.md`'s
/// camelCase rule for Tauri commands.
#[tauri::command(rename_all = "camelCase")]
pub fn import_omp_providers_from_live(state: State<'_, AppState>) -> Result<usize, String> {
    crate::services::provider::import_omp_providers_from_live(state.inner())
        .map_err(|e| e.to_string())
}

/// List provider ids present in the Omp live config. JS binding: `getOmpLiveProviderIds`.
#[tauri::command(rename_all = "camelCase")]
pub fn get_omp_live_provider_ids() -> Result<Vec<String>, String> {
    omp_config::get_providers()
        .map(|providers| providers.keys().cloned().collect())
        .map_err(|e| e.to_string())
}

/// Read Omp's `modelRoles` map (role → `provider/model`). JS binding: `getOmpModelRoles`.
#[tauri::command(rename_all = "camelCase")]
pub fn get_omp_model_roles() -> Result<Map<String, Value>, String> {
    omp_config::get_model_roles().map_err(|e| e.to_string())
}

/// Detected Omp CLI version (None when the `omp` binary is not installed).
/// JS binding: `getOmpVersion`.
#[tauri::command(rename_all = "camelCase")]
pub fn get_omp_version() -> Option<String> {
    omp_config::detect_omp_version()
}

/// Minimum Omp version CC Switch can safely write config for.
/// JS binding: `getOmpMinSupportedVersion`.
#[tauri::command(rename_all = "camelCase")]
pub fn get_omp_min_supported_version() -> String {
    omp_config::MIN_OMP_VERSION.to_string()
}

/// Assign a DB provider to an Omp model role: upserts the provider into
/// models.yml and points `modelRoles.<role>` at it, then records the provider
/// as the local current provider. JS binding: `setOmpProviderRole`.
#[tauri::command(rename_all = "camelCase")]
pub fn set_omp_provider_role(
    provider_id: String,
    role: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    omp_config::validate_role(&role).map_err(|e| e.to_string())?;

    let provider = state
        .db
        .get_provider_by_id(&provider_id, AppType::Omp.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            AppError::localized(
                "provider.not_found",
                format!("供应商 {provider_id} 不存在"),
                format!("Provider {provider_id} not found"),
            )
            .to_string()
        })?;

    omp_config::upsert_and_select(&provider.id, provider.settings_config.clone(), &role)
        .map_err(|e| e.to_string())?;

    crate::settings::set_current_provider(&AppType::Omp, Some(&provider_id))
        .map_err(|e| e.to_string())
}

/// Remove a role assignment from Omp's `modelRoles`. JS binding: `clearOmpModelRole`.
#[tauri::command(rename_all = "camelCase")]
pub fn clear_omp_model_role(role: String) -> Result<(), String> {
    omp_config::validate_role(&role).map_err(|e| e.to_string())?;
    omp_config::clear_model_role(&role).map_err(|e| e.to_string())
}
