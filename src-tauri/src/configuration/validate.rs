use super::{
    AppConfiguration, ConfigurationError, DisplaySettings, MAX_CONFIGURATION_BYTES,
    MAX_PRICES_BYTES,
};
impl DisplaySettings {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        for id in [&self.primary_buckets.claude, &self.primary_buckets.codex]
            .into_iter()
            .flatten()
        {
            if id.is_empty() || id.len() > 138 || id.chars().any(char::is_control) {
                return Err(ConfigurationError::InvalidConfiguration);
            }
        }
        Ok(())
    }
}
impl AppConfiguration {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if self.schema_version > 1 {
            return Err(ConfigurationError::UnsupportedSchema);
        }
        if self.schema_version != 1
            || self.monitoring.validate().is_err()
            || crate::alerts::validate_settings(&self.alerts).is_err()
            || !crate::metrics::valid_settings(&self.telemetry)
            || crate::storage::validate_retention(self.quota_retention_days).is_err()
        {
            return Err(ConfigurationError::InvalidConfiguration);
        }
        self.display.validate()?;
        if serde_json::to_vec(self)
            .map_err(|_| ConfigurationError::InvalidConfiguration)?
            .len()
            > MAX_CONFIGURATION_BYTES
            || serde_json::to_vec(&self.price_overrides)
                .map_err(|_| ConfigurationError::InvalidConfiguration)?
                .len()
                > MAX_PRICES_BYTES
        {
            return Err(ConfigurationError::TooLarge);
        }
        let mut registry = crate::pricing::PriceRegistry::bundled()
            .map_err(|_| ConfigurationError::StorageCorrupt)?;
        let mut ids = std::collections::HashSet::new();
        for price in &self.price_overrides {
            if !ids.insert(&price.model_id) {
                return Err(ConfigurationError::InvalidConfiguration);
            }
            registry
                .set_override(price.clone())
                .map_err(|_| ConfigurationError::InvalidConfiguration)?;
        }
        Ok(())
    }
}
