use super::super::AppState;
use crate::storage::PreferenceKey;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MonitorSettings {
    pub paused: bool,
    pub interval_minutes: u16,
}
impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            paused: false,
            interval_minutes: 5,
        }
    }
}
impl MonitorSettings {
    pub fn validate(&self) -> Result<(), MonitorError> {
        if ![5, 10, 15, 30].contains(&self.interval_minutes) {
            return Err(MonitorError::InvalidSettings);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MonitorError {
    SettingsUnavailable,
    InvalidSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSettingsView {
    pub settings: Option<MonitorSettings>,
    pub error: Option<MonitorError>,
    pub next_check_ms: Option<i64>,
    pub effective_interval_minutes: Option<u16>,
    pub last_clock_change_ms: Option<i64>,
    pub runtime_reason: Option<MonitorRuntimeReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MonitorRuntimeReason {
    Paused,
    SettingsUnavailable,
    NoQualifiedProviders,
}

#[derive(Default)]
pub(super) struct Runtime {
    pub next_check_ms: Option<i64>,
    pub effective_minutes: Option<u16>,
    pub last_clock_change_ms: Option<i64>,
    pub no_qualified_providers: bool,
    pub recovered_observation: bool,
}
pub(crate) struct MonitorControl {
    pub(super) changes: watch::Sender<u64>,
    pub(super) runtime: Mutex<Runtime>,
}
impl Default for MonitorControl {
    fn default() -> Self {
        let (changes, _) = watch::channel(0);
        Self {
            changes,
            runtime: Mutex::new(Runtime::default()),
        }
    }
}

impl AppState {
    pub fn monitor_settings(&self) -> MonitorSettingsView {
        let result = self
            .load_preference::<MonitorSettings>(PreferenceKey::Monitor)
            .map_err(|_| MonitorError::SettingsUnavailable)
            .and_then(|settings| {
                // Read/parse errors were propagated above. Only a genuinely
                // absent first-install preference receives the approved default.
                let settings = settings.unwrap_or_default();
                settings.validate()?;
                Ok(settings)
            });
        let (settings, error) = match result {
            Ok(settings) => (Some(settings), None),
            Err(error) => (None, Some(error)),
        };
        let runtime = self
            .monitor
            .runtime
            .lock()
            .expect("monitor runtime lock poisoned");
        let runtime_reason = if error.is_some() {
            Some(MonitorRuntimeReason::SettingsUnavailable)
        } else if settings.as_ref().is_some_and(|settings| settings.paused) {
            Some(MonitorRuntimeReason::Paused)
        } else if runtime.no_qualified_providers {
            Some(MonitorRuntimeReason::NoQualifiedProviders)
        } else {
            None
        };
        let active = runtime_reason.is_none();
        MonitorSettingsView {
            settings,
            error,
            next_check_ms: if active { runtime.next_check_ms } else { None },
            effective_interval_minutes: if active {
                runtime.effective_minutes
            } else {
                None
            },
            last_clock_change_ms: runtime.last_clock_change_ms,
            runtime_reason,
        }
    }
    pub fn save_monitor_settings(
        &self,
        settings: MonitorSettings,
    ) -> Result<MonitorSettingsView, MonitorError> {
        settings.validate()?;
        self.save_preference(PreferenceKey::Monitor, &settings)
            .map_err(|_| MonitorError::SettingsUnavailable)?;
        self.monitor_settings_changed();
        Ok(self.monitor_settings())
    }
    pub(crate) fn monitor_settings_changed(&self) {
        {
            let mut runtime = self
                .monitor
                .runtime
                .lock()
                .expect("monitor runtime lock poisoned");
            runtime.next_check_ms = None;
            runtime.effective_minutes = None;
            runtime.no_qualified_providers = false;
            runtime.recovered_observation = false;
        }
        self.monitor
            .changes
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        self.record_diagnostic(crate::diagnostics::EventCode::MonitorChanged, 0, 0);
        self.publish();
    }
}
