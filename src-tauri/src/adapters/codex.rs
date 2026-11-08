use super::AdapterError;
use serde_json::Value;
pub struct CodexAdapter;
impl super::AgentAdapter for CodexAdapter {
    fn agent_id(&self) -> crate::model::AgentId {
        crate::model::AgentId::Codex
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuotaWindow {
    pub bucket_id: String,
    pub window_minutes: Option<i64>,
    pub used_percent: Option<f64>,
    pub resets_at_ms: Option<i64>,
    pub observed_ms: i64,
}

pub fn normalize_codex_limits(
    payload: &Value,
    observed_ms: i64,
) -> Result<Vec<QuotaWindow>, AdapterError> {
    let object = payload.as_object().ok_or(AdapterError::Malformed)?;
    if observed_ms < 0 {
        return Err(AdapterError::OutOfRange);
    }
    let mut windows = Vec::new();
    if let Some(buckets) = object.get("rateLimitsByLimitId").filter(|v| !v.is_null()) {
        for (id, bucket) in buckets.as_object().ok_or(AdapterError::Malformed)? {
            add_windows(&mut windows, id, bucket, observed_ms)?;
        }
    } else if let Some(bucket) = object.get("rateLimits").filter(|v| !v.is_null()) {
        let id = match bucket.get("limitId").filter(|v| !v.is_null()) {
            Some(id) => id.as_str().ok_or(AdapterError::Malformed)?,
            None => "codex",
        };
        add_windows(&mut windows, id, bucket, observed_ms)?;
    }
    if windows.len() > 32 {
        return Err(AdapterError::OutOfRange);
    }
    Ok(windows)
}

fn optional_integer(value: Option<&Value>) -> Result<Option<i64>, AdapterError> {
    value
        .filter(|v| !v.is_null())
        .map(|v| v.as_i64().ok_or(AdapterError::Malformed))
        .transpose()
}

fn add_windows(
    windows: &mut Vec<QuotaWindow>,
    id: &str,
    bucket: &Value,
    observed_ms: i64,
) -> Result<(), AdapterError> {
    let bucket = bucket.as_object().ok_or(AdapterError::Malformed)?;
    if id.is_empty() || id.len() > 128 {
        return Err(AdapterError::Malformed);
    }
    for name in ["primary", "secondary"] {
        let Some(window) = bucket.get(name).filter(|v| !v.is_null()) else {
            continue;
        };
        let fields = window.as_object().ok_or(AdapterError::Malformed)?;
        let used_percent = fields
            .get("usedPercent")
            .filter(|v| !v.is_null())
            .map(|v| v.as_f64().ok_or(AdapterError::Malformed))
            .transpose()?;
        if used_percent.is_some_and(|v| !v.is_finite() || !(0.0..=100.0).contains(&v)) {
            return Err(AdapterError::OutOfRange);
        }
        let window_minutes = optional_integer(fields.get("windowDurationMins"))?;
        let resets_at = optional_integer(fields.get("resetsAt"))?;
        if window_minutes.is_some_and(|v| v <= 0 || v > 9_007_199_254_740_991)
            || resets_at.is_some_and(|v| v < 0)
        {
            return Err(AdapterError::OutOfRange);
        }
        let resets_at_ms = resets_at
            .map(|v| v.checked_mul(1000).ok_or(AdapterError::OutOfRange))
            .transpose()?;
        if resets_at_ms.is_some_and(|v| v > 8_640_000_000_000_000) {
            return Err(AdapterError::OutOfRange);
        }
        windows.push(QuotaWindow {
            bucket_id: format!("{id}:{name}"),
            window_minutes,
            used_percent,
            resets_at_ms,
            observed_ms,
        });
    }
    Ok(())
}
