use super::AppState;
use crate::configuration::{AppConfiguration, ConfigurationError, DisplaySettings};
use std::sync::Arc;

impl AppState {
    pub fn maintenance_status(
        &self,
    ) -> Result<crate::storage::MaintenanceStatus, ConfigurationError> {
        Ok(self
            .storage
            .lock()
            .expect("storage lock poisoned")
            .as_ref()
            .ok_or(ConfigurationError::StorageUnavailable)?
            .maintenance_status())
    }
    pub fn export_configuration(&self) -> Result<AppConfiguration, ConfigurationError> {
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_ref()
            .ok_or(ConfigurationError::StorageUnavailable)?
            .read_configuration()
    }
    pub fn display_settings(&self) -> Result<DisplaySettings, ConfigurationError> {
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_ref()
            .ok_or(ConfigurationError::StorageUnavailable)?
            .read_display_settings()
    }
    pub fn save_display_settings(
        &self,
        settings: DisplaySettings,
    ) -> Result<DisplaySettings, ConfigurationError> {
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_mut()
            .ok_or(ConfigurationError::StorageUnavailable)?
            .write_display_settings(&settings)?;
        self.publish();
        Ok(settings)
    }
    pub fn import_configuration(
        self: &Arc<Self>,
        configuration: AppConfiguration,
        confirmed: bool,
    ) -> Result<AppConfiguration, ConfigurationError> {
        if !confirmed {
            return Err(ConfigurationError::ConfirmationRequired);
        }
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_mut()
            .ok_or(ConfigurationError::StorageUnavailable)?
            .replace_configuration(&configuration, super::now_ms())?;
        self.monitor_settings_changed();
        self.reconcile_metrics();
        self.publish();
        let metrics = self
            .metrics_view(1)
            .map_err(|_| ConfigurationError::RuntimeUnavailable)?;
        let alerts = self
            .alert_settings()
            .map_err(|_| ConfigurationError::RuntimeUnavailable)?;
        if metrics.status.error.is_some()
            || (configuration.telemetry.enabled && !metrics.running)
            || (configuration.alerts.enabled
                && !matches!(alerts.permission, super::AlertPermission::Granted))
        {
            return Err(ConfigurationError::RuntimeUnavailable);
        }
        Ok(configuration)
    }
}
