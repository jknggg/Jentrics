use jentrics_core::adapters::{codex::normalize_codex_limits, AdapterError};
use serde_json::json;

#[test]
fn multi_bucket_view_wins_without_duplicate_legacy_samples() {
    let fixture = serde_json::from_str(include_str!("../../fixtures/codex-limits.json")).unwrap();
    let windows = normalize_codex_limits(&fixture, 1000).unwrap();
    assert_eq!(windows.len(), 3);
    assert_eq!(windows[0].bucket_id, "codex:primary");
    assert_eq!(windows[0].window_minutes, Some(15));
    assert_eq!(windows[0].used_percent, Some(25.0));
    assert_eq!(windows[0].resets_at_ms, Some(1_730_947_200_000));
    assert_eq!(windows[1].bucket_id, "codex:secondary");
    assert_eq!(windows[1].used_percent, None);
    assert_eq!(windows[1].resets_at_ms, None);
    assert_eq!(windows[2].bucket_id, "codex_other:primary");
    assert_eq!(windows[2].observed_ms, 1000);
}

#[test]
fn missing_quota_data_never_becomes_zero() {
    assert!(normalize_codex_limits(&json!({}), 1000).unwrap().is_empty());
    assert!(normalize_codex_limits(&json!({"rateLimits":null}), 1000)
        .unwrap()
        .is_empty());
    let result = normalize_codex_limits(&json!({"rateLimits":{"primary":{}}}), 1000).unwrap();
    assert_eq!(result[0].used_percent, None);
}

#[test]
fn malformed_and_out_of_range_values_are_rejected() {
    for primary in [
        json!({"usedPercent":-1}),
        json!({"usedPercent":101}),
        json!({"resetsAt":i64::MAX}),
        json!({"windowDurationMins":-3}),
    ] {
        assert_eq!(
            normalize_codex_limits(&json!({"rateLimits":{"primary":primary}}), 1),
            Err(AdapterError::OutOfRange)
        );
    }
    for payload in [
        json!({"rateLimits":false}),
        json!({"rateLimitsByLimitId":[]}),
        json!({"rateLimits":{"primary":{"usedPercent":"25"}}}),
    ] {
        assert_eq!(
            normalize_codex_limits(&payload, 1),
            Err(AdapterError::Malformed)
        );
    }
}
