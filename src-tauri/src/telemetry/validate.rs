use super::{Metric, Provider, TelemetryDelta, TelemetryError};

pub fn validate_delta(delta: &TelemetryDelta) -> Result<(), TelemetryError> {
    let fingerprint = delta
        .series_key
        .strip_prefix("defaultHasherV1:")
        .ok_or(TelemetryError::Malformed)?;
    if fingerprint.len() != 16 || !fingerprint.bytes().all(|value| value.is_ascii_hexdigit()) {
        return Err(TelemetryError::Malformed);
    }
    let parse_time = |text: &str| -> Result<u64, TelemetryError> {
        if text.is_empty() || text.len() > 20 || !text.bytes().all(|value| value.is_ascii_digit()) {
            return Err(TelemetryError::Malformed);
        }
        text.parse().map_err(|_| TelemetryError::OutOfRange)
    };
    let start = parse_time(&delta.start_ns)?;
    let end = parse_time(&delta.end_ns)?;
    let epoch = parse_time(&delta.counter_start_ns)?;
    if start == 0
        || end <= start
        || epoch == 0
        || epoch > start
        || end / 1_000_000 > i64::MAX as u64
        || delta.start_ms != (start / 1_000_000) as i64
        || delta.end_ms != (end / 1_000_000) as i64
    {
        return Err(TelemetryError::OutOfRange);
    }
    if !delta.value.is_finite()
        || !(0.0..=9_007_199_254_740_991.0).contains(&delta.value)
        || delta
            .count
            .is_some_and(|value| value > 9_007_199_254_740_991)
    {
        return Err(TelemetryError::OutOfRange);
    }
    if delta.model.as_ref().is_some_and(|value| {
        value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-:".contains(&byte))
    }) {
        return Err(TelemetryError::OutOfRange);
    }
    let valid = match (delta.provider, delta.metric) {
        (provider, Metric::Tokens) => {
            let allowed = match provider {
                Provider::Claude => ["input", "output", "cacheRead", "cacheCreation"].as_slice(),
                Provider::Gemini => ["input", "output", "thought", "cache", "tool"].as_slice(),
            };
            delta.count.is_none()
                && delta.value.fract() == 0.0
                && delta
                    .dimension
                    .as_deref()
                    .is_some_and(|dimension| allowed.contains(&dimension))
        }
        (Provider::Claude, Metric::CostUsd) => delta.count.is_none() && delta.dimension.is_none(),
        (Provider::Claude, Metric::ActiveSeconds) => {
            delta.count.is_none() && matches!(delta.dimension.as_deref(), Some("user" | "cli"))
        }
        (Provider::Gemini, Metric::RequestLatencyMs) => {
            delta.dimension.is_none()
                && delta
                    .count
                    .is_some_and(|count| count != 0 || delta.value == 0.0)
        }
        (Provider::Gemini, Metric::ApiRequests) => {
            delta.count.is_none()
                && delta.value.fract() == 0.0
                && matches!(
                    delta.dimension.as_deref(),
                    Some("success" | "error" | "unknown")
                )
        }
        _ => false,
    };
    if !valid {
        return Err(TelemetryError::UnsupportedShape);
    }
    Ok(())
}
