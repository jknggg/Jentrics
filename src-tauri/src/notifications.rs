use crate::{
    alerts::AlertSettings,
    state::{AlertError, AlertPermission, AlertSettingsView, AppState},
};
use std::sync::Arc;
use tauri_plugin_notification::NotificationExt;
pub fn install(app: &tauri::AppHandle, state: &Arc<AppState>) {
    let handle = app.clone();
    state.set_alert_sink(
        Arc::new(move |event| {
            handle
                .notification()
                .builder()
                .title(&event.title)
                .body(&event.message)
                .show()
                .map_err(|_| AlertError::DeliveryUnavailable)
        }),
        permission(app),
    );
}
fn permission(app: &tauri::AppHandle) -> AlertPermission {
    match app.notification().permission_state() {
        Ok(tauri::plugin::PermissionState::Granted) => AlertPermission::Granted,
        Ok(tauri::plugin::PermissionState::Denied) => AlertPermission::Denied,
        Ok(_) => AlertPermission::Prompt,
        Err(_) => AlertPermission::Unavailable,
    }
}
#[tauri::command]
pub fn get_alert_settings(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<AlertSettingsView, AlertError> {
    state.set_alert_permission(permission(&app));
    state.alert_settings()
}
#[tauri::command]
pub fn set_alert_settings(
    state: tauri::State<'_, Arc<AppState>>,
    settings: AlertSettings,
) -> Result<AlertSettingsView, AlertError> {
    state.save_alert_settings(settings)
}
#[tauri::command]
pub fn request_alert_permission(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<AlertSettingsView, AlertError> {
    app.notification()
        .request_permission()
        .map_err(|_| AlertError::PermissionUnavailable)?;
    state.set_alert_permission(permission(&app));
    state.alert_settings()
}
#[tauri::command]
pub fn send_test_notification(app: tauri::AppHandle) -> Result<(), AlertError> {
    if !matches!(permission(&app), AlertPermission::Granted) {
        return Err(AlertError::PermissionUnavailable);
    }
    app.notification()
        .builder()
        .title("Jentrics notification test")
        .body("Test notification only. Automatic alert settings have not changed.")
        .show()
        .map_err(|_| AlertError::DeliveryUnavailable)
}
