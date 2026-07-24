use tauri::State;

use crate::kimi_code_config;
use crate::store::AppState;

/// Import providers from Kimi Code live config.toml into the database.
#[tauri::command]
pub fn import_kimicode_providers_from_live(state: State<'_, AppState>) -> Result<usize, String> {
    crate::services::provider::import_kimicode_providers_from_live(state.inner())
        .map_err(|e| e.to_string())
}

/// List provider ids present in the Kimi Code live config.
#[tauri::command]
pub fn get_kimicode_live_provider_ids() -> Result<Vec<String>, String> {
    kimi_code_config::get_providers()
        .map(|providers| providers.keys().cloned().collect())
        .map_err(|e| e.to_string())
}

/// Read the current `default_model` from live config.
#[tauri::command]
pub fn get_kimicode_default_model() -> Result<Option<String>, String> {
    kimi_code_config::get_default_model().map_err(|e| e.to_string())
}

/// Provider id that owns the current `default_model` selection.
#[tauri::command]
pub fn get_kimicode_default_provider_id() -> Result<Option<String>, String> {
    kimi_code_config::get_default_provider_id().map_err(|e| e.to_string())
}
