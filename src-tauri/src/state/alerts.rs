use super::AppState;
use crate::{
    alerts::{
        evaluate_alerts, evaluate_monitor_error, validate_settings, AlertEvent, AlertKey,
        AlertSettings,
    },
    model::DashboardState,
    storage::PreferenceKey,
};
use serde::Serialize;
use std::sync::{Arc, Mutex};
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertError {
    SettingsUnavailable,
    DeliveryUnavailable,
    PermissionUnavailable,
}
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertPermission {
    Granted,
    Denied,
    Prompt,
    Unavailable,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertSettingsView {
    pub settings: AlertSettings,
    pub permission: AlertPermission,
    pub error: Option<AlertError>,
    pub windows_state: crate::alerts::WindowsNotificationState,
}
type AlertSink = Arc<dyn Fn(&AlertEvent) -> Result<(), AlertError> + Send + Sync>;
pub(super) struct AlertControl {
    sink: Mutex<Option<AlertSink>>,
    permission: Mutex<AlertPermission>,
    error: Mutex<Option<AlertError>>,
    gate: Mutex<()>,
}
impl Default for AlertControl {
    fn default() -> Self {
        Self {
            sink: Mutex::new(None),
            permission: Mutex::new(AlertPermission::Unavailable),
            error: Mutex::new(None),
            gate: Mutex::new(()),
        }
    }
}
impl AppState {
    pub fn set_alert_sink(&self, sink: AlertSink, permission: AlertPermission) {
        *self.alerts.sink.lock().expect("alert sink lock poisoned") = Some(sink);
        *self
            .alerts
            .permission
            .lock()
            .expect("alert permission lock poisoned") = permission;
    }
    pub fn set_alert_permission(&self, permission: AlertPermission) {
        *self
            .alerts
            .permission
            .lock()
            .expect("alert permission lock poisoned") = permission;
    }
    pub fn alert_settings(&self) -> Result<AlertSettingsView, AlertError> {
        let settings: AlertSettings = self
            .load_preference(PreferenceKey::Alerts)
            .map_err(|_| AlertError::SettingsUnavailable)?
            .unwrap_or_default();
        validate_settings(&settings).map_err(|_| AlertError::SettingsUnavailable)?;
        Ok(AlertSettingsView {
            windows_state: crate::alerts::windows_state(),
            settings,
            permission: *self
                .alerts
                .permission
                .lock()
                .expect("alert permission lock poisoned"),
            error: *self.alerts.error.lock().expect("alert error lock poisoned"),
        })
    }
    pub fn save_alert_settings(
        &self,
        settings: AlertSettings,
    ) -> Result<AlertSettingsView, AlertError> {
        validate_settings(&settings).map_err(|_| AlertError::SettingsUnavailable)?;
        self.save_preference(PreferenceKey::Alerts, &settings)
            .map_err(|_| AlertError::SettingsUnavailable)?;
        self.alert_settings()
    }
    pub(super) fn process_alerts(&self, previous: &DashboardState, current: &DashboardState) {
        let _gate = self
            .alerts
            .gate
            .lock()
            .expect("alert evaluation lock poisoned");
        let result = (|| {
            let settings = self.alert_settings()?;
            if !settings.settings.enabled {
                return Ok(());
            }
            if !matches!(settings.permission, AlertPermission::Granted) {
                return Err(AlertError::PermissionUnavailable);
            }
            let now = super::now_ms();
            let local = local_minute()?;
            let mut seen: Vec<AlertKey> = self
                .load_preference(PreferenceKey::AlertSeen)
                .map_err(|_| AlertError::SettingsUnavailable)?
                .unwrap_or_default();
            seen.retain(|key| {
                key.epoch_ms
                    .or(key.resets_at_ms)
                    .is_none_or(|time| time >= now - 90 * 86400000)
            });
            let mut events = evaluate_alerts(
                &settings.settings,
                &current.agents,
                &previous.agents,
                &seen,
                now,
                local,
            )
            .map_err(|_| AlertError::SettingsUnavailable)?;
            if let Some(event) = evaluate_monitor_error(
                &settings.settings,
                current.collection_error.is_some(),
                previous.collection_error.is_some(),
                &seen,
                now,
                local,
            )
            .map_err(|_| AlertError::SettingsUnavailable)?
            {
                events.push(event);
            }
            let sink = self
                .alerts
                .sink
                .lock()
                .expect("alert sink lock poisoned")
                .clone()
                .ok_or(AlertError::DeliveryUnavailable)?;
            for event in events {
                sink(&event)?;
                seen.push(event.key);
                self.save_preference(PreferenceKey::AlertSeen, &seen)
                    .map_err(|_| AlertError::SettingsUnavailable)?;
            }
            Ok(())
        })();
        *self.alerts.error.lock().expect("alert error lock poisoned") = result.err();
    }
}
#[cfg(windows)]
fn local_minute() -> Result<u16, AlertError> {
    let mut time = std::mem::MaybeUninit::uninit();
    unsafe {
        windows_sys::Win32::System::SystemInformation::GetLocalTime(time.as_mut_ptr());
        let time = time.assume_init();
        Ok(time.wHour * 60 + time.wMinute)
    }
}
#[cfg(not(windows))]
fn local_minute() -> Result<u16, AlertError> {
    Err(AlertError::SettingsUnavailable)
}
