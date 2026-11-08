use jentrics_core::{
    model::*,
    wake::{
        eligibility::{check, quota_effect},
        QuotaEffect, WakeReason,
    },
};
use serde_json::json;

fn snapshot(windows: serde_json::Value) -> DashboardState {
    serde_json::from_value(json!({"schemaVersion":1,"agents":[{"agentId":"codex","version":"0.165.0-alpha.1.2","availability":"detected","authState":"ready","capabilities":{"discovery":true,"authReadiness":true,"quotas":true,"usage":false,"sessionStart":false},"observedMs":1000,"diagnosticCode":null,"quota":{"source":"codexAppServer","observedMs":1000,"diagnosticCode":null,"windows":windows}}],"refreshing":false,"lastRefreshMs":1000,"storageError":null,"collectionError":null})).unwrap()
}
fn window(minutes: Option<i64>, reset: Option<i64>) -> serde_json::Value {
    json!({"bucketId":"codex:primary","windowMinutes":minutes,"usedPercent":0,"resetsAtMs":reset,"observedMs":1000})
}
#[test]
fn active_five_hour_window_and_positive_weekly_only_evidence_disable_wake() {
    assert_eq!(
        check(
            &snapshot(json!([window(Some(300), Some(9000))])),
            AgentId::Codex,
            1000
        ),
        Err(WakeReason::ActiveWindow)
    );
    assert_eq!(
        check(
            &snapshot(json!([window(Some(10080), Some(9000))])),
            AgentId::Codex,
            1000
        ),
        Err(WakeReason::WeeklyOnly)
    );
    assert_eq!(
        check(
            &snapshot(json!([window(Some(300), Some(900))])),
            AgentId::Codex,
            1000
        ),
        Ok(false)
    );
}
#[test]
fn missing_or_unknown_duration_is_not_misclassified_as_weekly_only() {
    for windows in [
        json!([]),
        json!([window(None, None)]),
        json!([window(Some(10080), Some(9000)), window(None, None)]),
    ] {
        assert_eq!(check(&snapshot(windows), AgentId::Codex, 1000), Ok(true));
    }
    let mut state = snapshot(json!([]));
    state.agents[0].quota = None;
    assert_eq!(check(&state, AgentId::Codex, 1000), Ok(true));
}
#[test]
fn stale_failed_or_unauthenticated_state_cannot_prepare_an_action() {
    let mut state = snapshot(json!([window(Some(300), Some(900))]));
    assert_eq!(
        check(&state, AgentId::Codex, 901001),
        Err(WakeReason::ProviderUnavailable)
    );
    state.collection_error = Some(DiagnosticCode::AuthUnavailable);
    assert_eq!(
        check(&state, AgentId::Codex, 1000),
        Err(WakeReason::ProviderUnavailable)
    );
    state.collection_error = None;
    state.agents[0].auth_state = AuthState::NotAuthenticated;
    assert_eq!(
        check(&state, AgentId::Codex, 1000),
        Err(WakeReason::SignInRequired)
    );
}
#[test]
fn newly_reported_reset_requires_fresh_post_request_evidence() {
    let before = snapshot(json!([window(Some(300), Some(900))]));
    let after = snapshot(json!([window(Some(300), Some(9000))]));
    assert_eq!(
        quota_effect(&before, &after, AgentId::Codex, 1000),
        QuotaEffect::NewWindowObserved
    );
    assert_eq!(
        quota_effect(&after, &after, AgentId::Codex, 1000),
        QuotaEffect::Unchanged
    );
    let mut failed = after.clone();
    failed.collection_error = Some(DiagnosticCode::AuthUnavailable);
    assert_eq!(
        quota_effect(&before, &failed, AgentId::Codex, 1000),
        QuotaEffect::Unverified
    );
    assert_eq!(
        quota_effect(&before, &snapshot(json!([])), AgentId::Codex, 1000),
        QuotaEffect::Unverified
    );
}
