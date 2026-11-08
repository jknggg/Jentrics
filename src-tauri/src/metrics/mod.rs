use crate::telemetry::TelemetryDelta;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetricSettings {
    pub enabled: bool,
    pub event_retention_days: u16,
    pub daily_retention_days: u16,
}
impl Default for MetricSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            event_retention_days: 30,
            daily_retention_days: 365,
        }
    }
}
pub fn valid_settings(value: &MetricSettings) -> bool {
    matches!(value.event_retention_days, 7 | 30 | 90)
        && matches!(value.daily_retention_days, 30 | 90 | 365)
        && value.daily_retention_days >= value.event_retention_days
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DailyMetric {
    pub day_ms: i64,
    pub provider: String,
    pub metric: String,
    pub dimension: Option<String>,
    pub model: Option<String>,
    pub value: f64,
    pub count: Option<i64>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricHistory {
    pub events: Vec<TelemetryDelta>,
    pub daily: Vec<DailyMetric>,
    pub truncated: bool,
}
