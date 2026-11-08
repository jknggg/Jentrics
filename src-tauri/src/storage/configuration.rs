use super::{preferences::PreferenceKey, settings, Storage, StorageError};
use crate::configuration::{AppConfiguration, ConfigurationError, DisplaySettings};
use serde::de::DeserializeOwned;

pub(super) fn configuration_error(error: StorageError) -> ConfigurationError {
    match error {
        StorageError::Corrupt | StorageError::Incompatible => ConfigurationError::StorageCorrupt,
        StorageError::InvalidState => ConfigurationError::InvalidConfiguration,
        StorageError::Unavailable => ConfigurationError::StorageUnavailable,
    }
}
impl Storage {
    fn configuration_preference<T: DeserializeOwned + Default>(
        &self,
        key: PreferenceKey,
    ) -> Result<T, ConfigurationError> {
        // Only a genuinely absent first-install preference gets its documented
        // default. Invalid JSON, invalid shapes and database errors propagate.
        self.read_preference(key)
            .map_err(configuration_error)?
            .map(|value| {
                serde_json::from_value(value).map_err(|_| ConfigurationError::StorageCorrupt)
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }
    pub fn read_display_settings(&self) -> Result<DisplaySettings, ConfigurationError> {
        let settings: DisplaySettings = self.configuration_preference(PreferenceKey::Display)?;
        settings
            .validate()
            .map_err(|_| ConfigurationError::StorageCorrupt)?;
        Ok(settings)
    }
    pub fn write_display_settings(
        &mut self,
        settings: &DisplaySettings,
    ) -> Result<(), ConfigurationError> {
        settings.validate()?;
        self.write_preference(
            PreferenceKey::Display,
            &serde_json::to_value(settings)
                .map_err(|_| ConfigurationError::InvalidConfiguration)?,
        )
        .map_err(configuration_error)
    }
    pub fn read_configuration(&self) -> Result<AppConfiguration, ConfigurationError> {
        let configuration = AppConfiguration {
            schema_version: 1,
            monitoring: self.configuration_preference(PreferenceKey::Monitor)?,
            alerts: self.configuration_preference(PreferenceKey::Alerts)?,
            telemetry: self.configuration_preference(PreferenceKey::Telemetry)?,
            quota_retention_days: self
                .history_settings()
                .map_err(configuration_error)?
                .retention_days,
            display: self.read_display_settings()?,
            price_overrides: self.configuration_preference(PreferenceKey::Prices)?,
        };
        configuration.validate().map_err(|error| match error {
            ConfigurationError::TooLarge => error,
            _ => ConfigurationError::StorageCorrupt,
        })?;
        Ok(configuration)
    }
    pub fn replace_configuration(
        &mut self,
        configuration: &AppConfiguration,
        now_ms: i64,
    ) -> Result<(), ConfigurationError> {
        configuration.validate()?;
        settings::validate_timestamp(now_ms).map_err(configuration_error)?;
        let rows = [
            (
                PreferenceKey::Monitor,
                serde_json::to_value(&configuration.monitoring),
            ),
            (
                PreferenceKey::Alerts,
                serde_json::to_value(&configuration.alerts),
            ),
            (
                PreferenceKey::Telemetry,
                serde_json::to_value(&configuration.telemetry),
            ),
            (
                PreferenceKey::Display,
                serde_json::to_value(&configuration.display),
            ),
            (
                PreferenceKey::Prices,
                serde_json::to_value(&configuration.price_overrides),
            ),
        ]
        .into_iter()
        .map(|(key, value)| {
            let encoded = serde_json::to_string(
                &value.map_err(|_| ConfigurationError::InvalidConfiguration)?,
            )
            .map_err(|_| ConfigurationError::InvalidConfiguration)?;
            if encoded.len() > 65536 {
                return Err(ConfigurationError::TooLarge);
            }
            Ok((key, encoded))
        })
        .collect::<Result<Vec<_>, _>>()?;
        let tx = self
            .connection
            .transaction()
            .map_err(|_| ConfigurationError::StorageUnavailable)?;
        // Corrupt settings must not be silently recreated by an import.
        settings::read_settings(&tx).map_err(configuration_error)?;
        for (key, encoded) in rows {
            tx.execute("INSERT INTO preferences VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", rusqlite::params![key.as_str(), encoded])
                .map_err(|_| ConfigurationError::StorageUnavailable)?;
        }
        tx.execute(
            "UPDATE app_settings SET retention_days=?1 WHERE id=1",
            [configuration.quota_retention_days],
        )
        .map_err(|_| ConfigurationError::StorageUnavailable)?;
        settings::prune(&tx, now_ms).map_err(configuration_error)?;
        super::metrics::prune_metrics(
            &tx,
            now_ms,
            configuration.telemetry.event_retention_days,
            configuration.telemetry.daily_retention_days,
        )
        .map_err(configuration_error)?;
        tx.commit()
            .map_err(|_| ConfigurationError::StorageUnavailable)?;
        self.finish_maintenance(super::MaintenanceOperation::ConfigurationImport);
        Ok(())
    }
}
