use crate::{
    configuration::{AppConfiguration, ConfigurationError, DisplaySettings},
    state::AppState,
};
use std::sync::Arc;

#[tauri::command]
pub fn export_configuration(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppConfiguration, ConfigurationError> {
    state.export_configuration()
}

#[tauri::command]
pub fn import_configuration(
    state: tauri::State<'_, Arc<AppState>>,
    configuration: AppConfiguration,
    confirmed: bool,
) -> Result<AppConfiguration, ConfigurationError> {
    state.inner().import_configuration(configuration, confirmed)
}

#[tauri::command]
pub fn get_display_settings(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DisplaySettings, ConfigurationError> {
    state.display_settings()
}

#[tauri::command]
pub fn set_display_settings(
    state: tauri::State<'_, Arc<AppState>>,
    settings: DisplaySettings,
) -> Result<DisplaySettings, ConfigurationError> {
    state.save_display_settings(settings)
}

#[tauri::command]
pub fn get_storage_maintenance(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::storage::MaintenanceStatus, ConfigurationError> {
    state.maintenance_status()
}

#[tauri::command]
pub fn get_adapter_catalog(
    state: tauri::State<'_, Arc<AppState>>,
) -> crate::adapters::AdapterCatalog {
    crate::adapters::catalog(&state.snapshot())
}

#[tauri::command]
pub fn get_diagnostics_status(
    state: tauri::State<'_, Arc<AppState>>,
) -> crate::diagnostics::DiagnosticStatus {
    state.diagnostics_status()
}

#[tauri::command]
pub fn take_settings_request(state: tauri::State<'_, Arc<AppState>>) -> bool {
    state.take_settings_request()
}
