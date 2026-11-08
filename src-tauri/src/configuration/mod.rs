use crate::{
    alerts::AlertSettings, metrics::MetricSettings, pricing::ModelPrice, state::MonitorSettings,
};
use serde::{Deserialize, Deserializer, Serialize};
mod validate;
pub const MAX_CONFIGURATION_BYTES: usize = 128 * 1024;
pub const MAX_PRICES_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigurationError {
    ConfirmationRequired,
    UnsupportedSchema,
    InvalidConfiguration,
    TooLarge,
    StorageUnavailable,
    StorageCorrupt,
    /// Settings committed; collector/notification readiness must be checked separately.
    RuntimeUnavailable,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrimaryBuckets {
    #[serde(deserialize_with = "required_nullable")]
    pub claude: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub codex: Option<String>,
}
fn required_nullable<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::deserialize(deserializer)
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisplaySettings {
    pub primary_buckets: PrimaryBuckets,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppConfiguration {
    pub schema_version: u8,
    pub monitoring: MonitorSettings,
    pub alerts: AlertSettings,
    pub telemetry: MetricSettings,
    pub quota_retention_days: u16,
    pub display: DisplaySettings,
    pub price_overrides: Vec<ModelPrice>,
}
impl AppConfiguration {
    pub fn from_json(encoded: &str) -> Result<Self, ConfigurationError> {
        if encoded.len() > MAX_CONFIGURATION_BYTES {
            return Err(ConfigurationError::TooLarge);
        }
        let value: serde_json::Value =
            serde_json::from_str(encoded).map_err(|_| ConfigurationError::InvalidConfiguration)?;
        if value
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|v| v > 1)
        {
            return Err(ConfigurationError::UnsupportedSchema);
        }
        let config: Self =
            serde_json::from_value(value).map_err(|_| ConfigurationError::InvalidConfiguration)?;
        config.validate()?;
        Ok(config)
    }
}
