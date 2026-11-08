use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
mod normalize;
pub use normalize::normalize_response;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageError {
    UnsupportedVersion,
    CapabilityUnavailable,
    Unavailable,
    Malformed,
    OutOfRange,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsageSnapshot {
    pub data: Option<CodexAccountUsage>,
    pub error: Option<UsageError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageSource {
    CodexAppServer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageScope {
    Account,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsageSummary {
    pub lifetime_tokens: Option<u64>,
    pub peak_daily_tokens: Option<u64>,
    pub longest_running_turn_sec: Option<u64>,
    pub current_streak_days: Option<u64>,
    pub longest_streak_days: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsageBucket {
    pub start_date: String,
    pub tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAccountUsage {
    pub source: UsageSource,
    pub scope: UsageScope,
    pub observed_ms: i64,
    pub summary: AccountUsageSummary,
    pub daily_usage_buckets: Option<Vec<DailyUsageBucket>>,
}

/// Usage qualification is independent from the older quota-only profiles.
pub fn usage_supported(version: &str) -> bool {
    matches!(
        version,
        "0.165.0-alpha.1.2" | "0.166.0-alpha.5.1" | "0.167.0-alpha.3.2"
    )
}

/// Omitting params guarantees this read cannot request thread-specific usage.
pub fn request(version: &str, id: u64) -> Result<Value, UsageError> {
    if !usage_supported(version) {
        return Err(UsageError::UnsupportedVersion);
    }
    Ok(json!({"id":id,"method":"account/usage/read"}))
}
