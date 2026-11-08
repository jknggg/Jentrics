use super::{
    AccountUsageSummary, CodexAccountUsage, DailyUsageBucket, UsageError, UsageScope, UsageSource,
};
use serde_json::Value;

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

fn optional_number(value: Option<&Value>) -> Result<Option<u64>, UsageError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    if value.as_i64().is_some_and(|value| value < 0) {
        return Err(UsageError::OutOfRange);
    }
    let value = value.as_u64().ok_or(UsageError::Malformed)?;
    if value > MAX_SAFE_INTEGER {
        return Err(UsageError::OutOfRange);
    }
    Ok(Some(value))
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = value[..4].parse::<u16>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u8>() else {
        return false;
    };
    let Ok(day) = value[8..].parse::<u8>() else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year > 0 && (1..=days).contains(&day)
}

/// Extract only the account summary and daily numeric activity. Identity,
/// thread usage, provider errors and future unrelated fields are discarded.
pub fn normalize_response(
    envelope: &Value,
    observed_ms: i64,
) -> Result<CodexAccountUsage, UsageError> {
    if !(0..=8_640_000_000_000_000).contains(&observed_ms) {
        return Err(UsageError::OutOfRange);
    }
    let envelope = envelope.as_object().ok_or(UsageError::Malformed)?;
    if let Some(error) = envelope.get("error") {
        if envelope.contains_key("result") {
            return Err(UsageError::Malformed);
        }
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .ok_or(UsageError::Malformed)?;
        return Err(if code == -32601 {
            UsageError::CapabilityUnavailable
        } else {
            UsageError::Unavailable
        });
    }
    let result = envelope
        .get("result")
        .and_then(Value::as_object)
        .ok_or(UsageError::Malformed)?;
    let summary = result
        .get("summary")
        .and_then(Value::as_object)
        .ok_or(UsageError::Malformed)?;
    let summary = AccountUsageSummary {
        lifetime_tokens: optional_number(summary.get("lifetimeTokens"))?,
        peak_daily_tokens: optional_number(summary.get("peakDailyTokens"))?,
        longest_running_turn_sec: optional_number(summary.get("longestRunningTurnSec"))?,
        current_streak_days: optional_number(summary.get("currentStreakDays"))?,
        longest_streak_days: optional_number(summary.get("longestStreakDays"))?,
    };
    let daily_usage_buckets = match result
        .get("dailyUsageBuckets")
        .filter(|value| !value.is_null())
    {
        None => None,
        Some(value) => {
            let mut buckets = Vec::new();
            for bucket in value.as_array().ok_or(UsageError::Malformed)? {
                let bucket = bucket.as_object().ok_or(UsageError::Malformed)?;
                let start_date = bucket
                    .get("startDate")
                    .and_then(Value::as_str)
                    .filter(|value| valid_date(value))
                    .ok_or(UsageError::Malformed)?;
                let tokens = optional_number(bucket.get("tokens"))?.ok_or(UsageError::Malformed)?;
                buckets.push(DailyUsageBucket {
                    start_date: start_date.to_owned(),
                    tokens,
                });
            }
            Some(buckets)
        }
    };
    Ok(CodexAccountUsage {
        source: UsageSource::CodexAppServer,
        scope: UsageScope::Account,
        observed_ms,
        summary,
        daily_usage_buckets,
    })
}
