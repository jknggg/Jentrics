use jentrics_core::{
    alerts::{
        evaluate_monitor_error, AlertKey, AlertKind, AlertPolicyError, AlertSettings, QuietHours,
    },
    model::AgentId,
};
const NOW: i64 = 1_800_000_000_000;
fn enabled() -> AlertSettings {
    AlertSettings {
        enabled: true,
        ..Default::default()
    }
}
#[test]
fn new_global_failure_has_monitor_scope_and_fixed_sanitized_message() {
    let event = evaluate_monitor_error(&enabled(), true, false, &[], NOW, 600)
        .unwrap()
        .unwrap();
    assert_eq!(event.key.agent_id, None);
    assert_eq!(event.key.kind, AlertKind::MonitoringError);
    assert_eq!(event.key.epoch_ms, Some(NOW));
    assert_eq!(event.key.resets_at_ms, None);
    assert!(event.title.contains("Jentrics"));
    assert!(event.message.contains("monitoring"));
    assert!(!event.message.contains("Codex"));
    assert!(!event.message.contains("Claude"));
}
#[test]
fn global_errors_are_opt_in_episode_based_and_deduplicated_after_delivery() {
    assert!(
        evaluate_monitor_error(&AlertSettings::default(), true, false, &[], NOW, 600)
            .unwrap()
            .is_none()
    );
    let mut settings = enabled();
    settings.errors = false;
    assert!(
        evaluate_monitor_error(&settings, true, false, &[], NOW, 600)
            .unwrap()
            .is_none()
    );
    let settings = enabled();
    assert!(
        evaluate_monitor_error(&settings, false, false, &[], NOW, 600)
            .unwrap()
            .is_none()
    );
    assert!(evaluate_monitor_error(&settings, true, true, &[], NOW, 600)
        .unwrap()
        .is_none());
    let event = evaluate_monitor_error(&settings, true, false, &[], NOW, 600)
        .unwrap()
        .unwrap();
    assert!(evaluate_monitor_error(
        &settings,
        true,
        false,
        std::slice::from_ref(&event.key),
        NOW,
        600
    )
    .unwrap()
    .is_none());
    let next = evaluate_monitor_error(&settings, true, false, &[event.key], NOW + 1, 600)
        .unwrap()
        .unwrap();
    assert_eq!(next.key.epoch_ms, Some(NOW + 1));
}
#[test]
fn quiet_global_errors_are_suppressed_without_a_catchup_queue() {
    let mut settings = enabled();
    settings.quiet_hours = Some(QuietHours {
        start_minutes: 1320,
        end_minutes: 420,
    });
    assert!(evaluate_monitor_error(&settings, true, false, &[], NOW, 60)
        .unwrap()
        .is_none());
    assert!(
        evaluate_monitor_error(&settings, true, true, &[], NOW + 1, 600)
            .unwrap()
            .is_none()
    );
    assert!(
        evaluate_monitor_error(&settings, true, false, &[], NOW + 2, 600)
            .unwrap()
            .is_some()
    );
}
#[test]
fn global_and_provider_policies_share_invalid_clock_and_quiet_settings_errors() {
    assert_eq!(
        evaluate_monitor_error(&enabled(), true, false, &[], -1, 600),
        Err(AlertPolicyError::InvalidClock)
    );
    assert_eq!(
        evaluate_monitor_error(&enabled(), true, false, &[], NOW, 1440),
        Err(AlertPolicyError::InvalidLocalMinute)
    );
    let mut settings = enabled();
    settings.quiet_hours = Some(QuietHours {
        start_minutes: 60,
        end_minutes: 60,
    });
    assert_eq!(
        evaluate_monitor_error(&settings, true, false, &[], NOW, 600),
        Err(AlertPolicyError::InvalidSettings)
    );
}
#[test]
fn provider_dedup_json_remains_compatible_with_optional_monitor_scope() {
    let key:AlertKey=serde_json::from_str(r#"{"agentId":"codex","bucketId":"primary","resetsAtMs":1800000000000,"kind":"threshold95","epochMs":null}"#).unwrap();
    assert_eq!(key.agent_id, Some(AgentId::Codex));
    assert_eq!(serde_json::to_value(key).unwrap()["agentId"], "codex");
}
