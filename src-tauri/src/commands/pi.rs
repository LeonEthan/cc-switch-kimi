use tauri::State;

use crate::pi_config;
use crate::store::AppState;

/// Import providers from Pi live config (`models.json` + `auth.json`) into the database.
///
/// Tauri 2 binds command names on the JS side; `rename_all = "camelCase"` makes
/// this `importPiProvidersFromLive` in JavaScript — matching `CONTRIBUTING.md`'s
/// camelCase rule for Tauri commands.
#[tauri::command(rename_all = "camelCase")]
pub fn import_pi_providers_from_live(state: State<'_, AppState>) -> Result<usize, String> {
    crate::services::provider::import_pi_providers_from_live(state.inner())
        .map_err(|e| e.to_string())
}

/// List provider ids present in the Pi live config. JS binding: `getPiLiveProviderIds`.
#[tauri::command(rename_all = "camelCase")]
pub fn get_pi_live_provider_ids() -> Result<Vec<String>, String> {
    pi_config::get_providers()
        .map(|providers| providers.keys().cloned().collect())
        .map_err(|e| e.to_string())
}

/// Read the current `defaultModel` from Pi's settings.json. JS binding: `getPiDefaultModel`.
#[tauri::command(rename_all = "camelCase")]
pub fn get_pi_default_model() -> Result<Option<String>, String> {
    pi_config::get_default_model().map_err(|e| e.to_string())
}

/// Provider id that owns the current default selection (`defaultProvider`).
/// JS binding: `getPiDefaultProviderId`.
#[tauri::command(rename_all = "camelCase")]
pub fn get_pi_default_provider_id() -> Result<Option<String>, String> {
    pi_config::get_default_provider_id().map_err(|e| e.to_string())
}

/// Detected Pi CLI version (None when the `pi` binary is not installed). JS binding: `getPiVersion`.
#[tauri::command(rename_all = "camelCase")]
pub fn get_pi_version() -> Option<String> {
    pi_config::detect_pi_version()
}

/// Minimum Pi version CC Switch can safely write config for. JS binding: `getPiMinSupportedVersion`.
#[tauri::command(rename_all = "camelCase")]
pub fn get_pi_min_supported_version() -> String {
    pi_config::MIN_PI_VERSION.to_string()
}
