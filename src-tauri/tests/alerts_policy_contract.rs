use alerts::{evaluate_alerts, AlertKind, AlertSettings, QuietHours};
use jentrics_core::{adapters::codex::QuotaWindow, alerts, model};
use model::{
    AgentId, AgentState, AuthState, Availability, Capabilities, DiagnosticCode, QuotaSnapshot,
    QuotaSource,
};

const NOW: i64 = 1_800_000;
fn sample(used: Option<f64>, reset: Option<i64>) -> AgentState {
    AgentState {
        agent_id: AgentId::Codex,
        version: None,
        availability: Availability::Detected,
        auth_state: AuthState::Ready,
        capabilities: Capabilities {
            quotas: true,
            ..Default::default()
        },
        observed_ms: NOW,
        diagnostic_code: None,
        account_usage: None,
        quota: Some(QuotaSnapshot {
            source: QuotaSource::CodexAppServer,
            observed_ms: NOW,
            diagnostic_code: None,
            windows: vec![QuotaWindow {
                bucket_id: "primary".into(),
                window_minutes: Some(300),
                used_percent: used,
                resets_at_ms: reset,
                observed_ms: NOW,
            }],
        }),
    }
}
fn settings() -> AlertSettings {
    AlertSettings {
        enabled: true,
        threshold80: false,
        threshold95: false,
        near_reset: false,
        expected_reset: false,
        restored: false,
        errors: false,
        quiet_hours: None,
    }
}
fn evaluate(
    settings: &AlertSettings,
    agents: &[AgentState],
    previous: &[AgentState],
    seen: &[alerts::AlertKey],
) -> Vec<alerts::AlertEvent> {
    evaluate_alerts(settings, agents, previous, seen, NOW, 600).unwrap()
}

#[test]
fn default_is_off_and_invalid_settings_or_clock_fail_visibly() {
    assert!(!AlertSettings::default().enabled);
    assert!(evaluate(
        &AlertSettings::default(),
        &[sample(Some(99.0), Some(NOW + 300_000))],
        &[],
        &[]
    )
    .is_empty());
    let mut config = settings();
    config.quiet_hours = Some(QuietHours {
        start_minutes: 1440,
        end_minutes: 60,
    });
    assert!(alerts::validate_settings(&config).is_err());
    config.quiet_hours = Some(QuietHours {
        start_minutes: 60,
        end_minutes: 60,
    });
    assert!(alerts::validate_settings(&config).is_err());
    assert!(evaluate_alerts(&settings(), &[], &[], &[], -1, 600).is_err());
    assert!(evaluate_alerts(&settings(), &[], &[], &[], NOW, 1440).is_err());
}

#[test]
fn thresholds_use_reported_values_and_deduplicate_per_provider_bucket_reset() {
    let mut config = settings();
    config.threshold80 = true;
    config.threshold95 = true;
    let mut current = sample(Some(80.0), Some(NOW + 3_600_000));
    let first = evaluate(&config, std::slice::from_ref(&current), &[], &[]);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].key.kind, AlertKind::Threshold80);
    let seen = vec![first[0].key.clone()];
    assert!(evaluate(&config, std::slice::from_ref(&current), &[], &seen).is_empty());
    current.quota.as_mut().unwrap().windows[0].used_percent = Some(95.0);
    let high = evaluate(&config, std::slice::from_ref(&current), &[], &seen);
    assert_eq!(high.len(), 1);
    assert_eq!(high[0].key.kind, AlertKind::Threshold95);
    current.quota.as_mut().unwrap().windows[0].used_percent = Some(85.0);
    assert!(evaluate(
        &config,
        std::slice::from_ref(&current),
        &[],
        &[high[0].key.clone()]
    )
    .is_empty());
    current.agent_id = AgentId::Claude;
    assert_eq!(
        evaluate(&config, std::slice::from_ref(&current), &[], &seen).len(),
        1
    );
    current.agent_id = AgentId::Codex;
    current.quota.as_mut().unwrap().windows[0].bucket_id = "other".into();
    assert_eq!(
        evaluate(&config, std::slice::from_ref(&current), &[], &seen).len(),
        1
    );
    current.quota.as_mut().unwrap().windows[0].bucket_id = "primary".into();
    current.quota.as_mut().unwrap().windows[0].resets_at_ms = Some(NOW + 7_200_000);
    assert_eq!(evaluate(&config, &[current], &[], &seen).len(), 1);
}

#[test]
fn ninety_five_suppresses_eighty_and_unknown_or_stale_values_do_not_alert() {
    let mut config = settings();
    config.threshold80 = true;
    config.threshold95 = true;
    let first = evaluate(
        &config,
        &[sample(Some(99.0), Some(NOW + 3_600_000))],
        &[],
        &[],
    );
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].key.kind, AlertKind::Threshold95);
    for kind in ["null", "reset", "duration", "stale", "future"] {
        let mut current = sample(Some(99.0), Some(NOW + 3_600_000));
        let quota = current.quota.as_mut().unwrap();
        match kind {
            "null" => quota.windows[0].used_percent = None,
            "reset" => quota.windows[0].resets_at_ms = None,
            "duration" => quota.windows[0].window_minutes = None,
            "stale" => {
                quota.observed_ms = NOW - 900_000;
                quota.windows[0].observed_ms = quota.observed_ms;
            }
            "future" => {
                quota.observed_ms = NOW + 1;
                quota.windows[0].observed_ms = quota.observed_ms;
            }
            _ => unreachable!(),
        }
        assert!(evaluate(&config, &[current], &[], &[]).is_empty(), "{kind}");
    }
}

#[test]
fn near_reset_is_ten_minutes_and_time_passed_never_claims_capacity() {
    let mut config = settings();
    config.near_reset = true;
    config.expected_reset = true;
    let near = evaluate(
        &config,
        &[sample(Some(90.0), Some(NOW + 600_000))],
        &[],
        &[],
    );
    assert_eq!(near[0].key.kind, AlertKind::NearReset);
    assert!(evaluate(
        &config,
        &[sample(Some(90.0), Some(NOW + 600_001))],
        &[],
        &[]
    )
    .is_empty());
    let mut old = sample(Some(100.0), Some(NOW - 1));
    old.quota.as_mut().unwrap().observed_ms = NOW - 1000;
    old.quota.as_mut().unwrap().windows[0].observed_ms = NOW - 1000;
    let passed = evaluate(&config, &[old], &[], &[]);
    assert_eq!(passed[0].key.kind, AlertKind::ResetExpected);
    assert!(passed[0].message.contains("time passed"));
    assert!(passed[0].message.contains("verify"));
    assert!(!passed[0].message.contains("available"));
}

#[test]
fn restored_requires_fresh_post_reset_reported_room_and_advanced_future_reset() {
    let mut config = settings();
    config.restored = true;
    let mut previous = sample(Some(100.0), Some(NOW - 1000));
    previous.observed_ms = NOW - 2000;
    previous.quota.as_mut().unwrap().observed_ms = NOW - 2000;
    previous.quota.as_mut().unwrap().windows[0].observed_ms = NOW - 2000;
    let current = sample(Some(5.0), Some(NOW + 18_000_000));
    let result = evaluate(
        &config,
        std::slice::from_ref(&current),
        std::slice::from_ref(&previous),
        &[],
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key.kind, AlertKind::QuotaRestored);
    assert!(result[0].message.contains("reported"));
    assert!(!result[0].message.contains("100%"));
    for kind in [
        "noPrevious",
        "sameReset",
        "oldObservation",
        "noRoom",
        "unknown",
    ] {
        let mut next = current.clone();
        match kind {
            "sameReset" => next.quota.as_mut().unwrap().windows[0].resets_at_ms = Some(NOW - 1000),
            "oldObservation" => next.quota.as_mut().unwrap().windows[0].observed_ms = NOW - 2000,
            "noRoom" => next.quota.as_mut().unwrap().windows[0].used_percent = Some(99.0),
            "unknown" => next.quota.as_mut().unwrap().windows[0].used_percent = None,
            _ => (),
        }
        let prior = if kind == "noPrevious" {
            vec![]
        } else {
            vec![previous.clone()]
        };
        assert!(evaluate(&config, &[next], &prior, &[]).is_empty(), "{kind}");
    }
}

#[test]
fn quiet_hours_handle_midnight_and_do_not_consume_dedup_keys() {
    let mut config = settings();
    config.threshold80 = true;
    config.quiet_hours = Some(QuietHours {
        start_minutes: 1320,
        end_minutes: 420,
    });
    let current = sample(Some(90.0), Some(NOW + 3_600_000));
    for minute in [1320, 1439, 0, 419] {
        assert!(evaluate_alerts(
            &config,
            std::slice::from_ref(&current),
            &[],
            &[],
            NOW,
            minute
        )
        .unwrap()
        .is_empty());
    }
    for minute in [420, 600, 1319] {
        assert_eq!(
            evaluate_alerts(
                &config,
                std::slice::from_ref(&current),
                &[],
                &[],
                NOW,
                minute
            )
            .unwrap()
            .len(),
            1
        );
    }
    config.quiet_hours = Some(QuietHours {
        start_minutes: 60,
        end_minutes: 120,
    });
    assert!(evaluate_alerts(&config, &[current], &[], &[], NOW, 90)
        .unwrap()
        .is_empty());
}

#[test]
fn quota_errors_notify_once_per_episode_and_never_turn_missing_data_into_errors() {
    let mut config = settings();
    config.errors = true;
    let clean = sample(Some(20.0), Some(NOW + 3_600_000));
    let mut failed = clean.clone();
    failed.quota.as_mut().unwrap().diagnostic_code = Some(DiagnosticCode::QuotaUnavailable);
    let first = evaluate(
        &config,
        std::slice::from_ref(&failed),
        std::slice::from_ref(&clean),
        &[],
    );
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].key.kind, AlertKind::QuotaError);
    assert_eq!(first[0].key.epoch_ms, Some(NOW));
    failed.observed_ms += 1;
    failed.quota.as_mut().unwrap().observed_ms += 1;
    assert!(evaluate_alerts(
        &config,
        std::slice::from_ref(&failed),
        std::slice::from_ref(&failed),
        &[],
        NOW + 1,
        600
    )
    .unwrap()
    .is_empty());
    let next = evaluate_alerts(
        &config,
        &[failed],
        std::slice::from_ref(&clean),
        &[first[0].key.clone()],
        NOW + 1,
        600,
    )
    .unwrap();
    assert_eq!(next.len(), 1);
    assert_ne!(first[0].key, next[0].key);
    let mut unknown = clean;
    unknown.quota = None;
    assert!(evaluate(&config, &[unknown], &[], &[]).is_empty());
}
