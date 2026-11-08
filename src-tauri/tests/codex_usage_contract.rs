use jentrics_core::codex_usage::{normalize_response, request, usage_supported, UsageError};
use serde_json::json;

#[test]
fn account_summary_keeps_numeric_only_account_scope() {
    let value = json!({"id":7,"result":{"summary":{"lifetimeTokens":1234567,"peakDailyTokens":45678,"longestRunningTurnSec":540,"currentStreakDays":8,"longestStreakDays":14,"email":"private@example.invalid"},"dailyUsageBuckets":[{"startDate":"2026-06-18","tokens":12345}],"threadUsage":{"threadId":"private-thread","groups":[]},"credential":"private-secret"}});
    let usage = normalize_response(&value, 1000).unwrap();
    assert_eq!(usage.summary.lifetime_tokens, Some(1234567));
    assert_eq!(usage.summary.peak_daily_tokens, Some(45678));
    assert_eq!(usage.summary.longest_running_turn_sec, Some(540));
    assert_eq!(usage.summary.current_streak_days, Some(8));
    assert_eq!(usage.summary.longest_streak_days, Some(14));
    assert_eq!(usage.daily_usage_buckets.as_ref().unwrap()[0].tokens, 12345);
    let encoded = serde_json::to_value(&usage).unwrap();
    assert_eq!(encoded["source"], "codexAppServer");
    assert_eq!(encoded["scope"], "account");
    assert_eq!(encoded["observedMs"], 1000);
    for forbidden in [
        "private@",
        "private-thread",
        "private-secret",
        "threadUsage",
        "email",
        "credential",
    ] {
        assert!(!encoded.to_string().contains(forbidden));
        assert!(!format!("{usage:?}").contains(forbidden));
    }
}

#[test]
fn null_missing_empty_and_zero_are_distinct() {
    let unknown = normalize_response(
        &json!({"result":{"summary":{"lifetimeTokens":null},"dailyUsageBuckets":null}}),
        0,
    )
    .unwrap();
    assert_eq!(unknown.summary.lifetime_tokens, None);
    assert_eq!(unknown.summary.current_streak_days, None);
    assert_eq!(unknown.daily_usage_buckets, None);
    let missing = normalize_response(&json!({"result":{"summary":{}}}), 0).unwrap();
    assert_eq!(unknown, missing);
    let zero = normalize_response(
        &json!({"result":{"summary":{"lifetimeTokens":0},"dailyUsageBuckets":[]}}),
        0,
    )
    .unwrap();
    assert_eq!(zero.summary.lifetime_tokens, Some(0));
    assert_eq!(zero.daily_usage_buckets, Some(vec![]));
}

#[test]
fn usage_requests_are_version_gated_fixed_and_never_thread_scoped() {
    let command = request("0.165.0-alpha.1.2", 7).unwrap();
    assert_eq!(command, json!({"id":7,"method":"account/usage/read"}));
    assert!(usage_supported("0.165.0-alpha.1.2"));
    assert_eq!(
        request("0.166.0-alpha.5.1", 7).unwrap(),
        json!({"id":7,"method":"account/usage/read"})
    );
    assert_eq!(
        request("0.167.0-alpha.3.2", 7).unwrap(),
        json!({"id":7,"method":"account/usage/read"})
    );
    for version in [
        "0.162.1",
        "0.165.0",
        "0.165.0-alpha.1.3",
        "0.166.0",
        "0.166.0-alpha.5.2",
        "0.167.0",
        "0.167.0-alpha.3.3",
        "unknown",
        "",
    ] {
        assert!(!usage_supported(version));
        assert_eq!(request(version, 7), Err(UsageError::UnsupportedVersion));
    }
}

#[test]
fn capability_and_service_errors_remain_explicit_without_raw_text() {
    assert_eq!(
        normalize_response(
            &json!({"error":{"code":-32601,"message":"private-method"}}),
            1000
        ),
        Err(UsageError::CapabilityUnavailable)
    );
    assert_eq!(
        normalize_response(
            &json!({"error":{"code":-32000,"message":"private-credential"}}),
            1000
        ),
        Err(UsageError::Unavailable)
    );
    assert_eq!(
        normalize_response(&json!({"error":{"message":"private-credential"}}), 1000),
        Err(UsageError::Malformed)
    );
    assert_eq!(
        normalize_response(
            &json!({"error":{"code":-32601},"result":{"summary":{}}}),
            1000
        ),
        Err(UsageError::Malformed)
    );
}

#[test]
fn malformed_or_out_of_range_numbers_cannot_become_zero() {
    for payload in [
        json!(null),
        json!({}),
        json!({"result":null}),
        json!({"result":{}}),
        json!({"result":{"summary":null}}),
        json!({"result":{"summary":[]}}),
        json!({"result":{"summary":{"lifetimeTokens":"0"}}}),
        json!({"result":{"summary":{"lifetimeTokens":1.5}}}),
        json!({"result":{"summary":{},"dailyUsageBuckets":{}}}),
        json!({"result":{"summary":{},"dailyUsageBuckets":[{"startDate":"2026-01-01"}]}}),
    ] {
        assert_eq!(
            normalize_response(&payload, 1000),
            Err(UsageError::Malformed),
            "{payload}"
        );
    }
    for number in [json!(-1), json!(9007199254740992_u64)] {
        assert_eq!(
            normalize_response(
                &json!({"result":{"summary":{"lifetimeTokens":number}}}),
                1000
            ),
            Err(UsageError::OutOfRange)
        );
        assert_eq!(
            normalize_response(
                &json!({"result":{"summary":{},"dailyUsageBuckets":[{"startDate":"2026-01-01","tokens":number}]}}),
                1000
            ),
            Err(UsageError::OutOfRange)
        );
    }
    for observed in [-1, i64::MAX] {
        assert_eq!(
            normalize_response(&json!({"result":{"summary":{}}}), observed),
            Err(UsageError::OutOfRange)
        );
    }
}

#[test]
fn daily_dates_validate_calendar_and_never_retain_arbitrary_text() {
    for date in [
        "private-path",
        "2026-02-29",
        "2026-13-01",
        "2026-01-00",
        "2026-04-31",
        "2026-1-01",
        "",
    ] {
        assert_eq!(
            normalize_response(
                &json!({"result":{"summary":{},"dailyUsageBuckets":[{"startDate":date,"tokens":0}]}}),
                1000
            ),
            Err(UsageError::Malformed)
        );
    }
    let valid=normalize_response(&json!({"result":{"summary":{},"dailyUsageBuckets":[{"startDate":"2024-02-29","tokens":0}]}}),1000).unwrap();
    assert_eq!(
        valid.daily_usage_buckets.unwrap()[0].start_date,
        "2024-02-29"
    );
}
