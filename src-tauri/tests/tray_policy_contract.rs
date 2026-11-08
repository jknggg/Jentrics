use jentrics_core::{
    model::*,
    state::{AppState, Collector, MonitorSettings},
    storage::Storage,
    tray_policy::{pause_action, view},
};
use std::{future::Future, pin::Pin, sync::Arc};
struct Empty;
impl Collector for Empty {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async { Ok(vec![]) })
    }
}
#[test]
fn unavailable_quota_is_not_zero_and_pause_action_preserves_interval() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(Storage::open(&dir.path().join("db")), Arc::new(Empty));
    let monitor = state.monitor_settings();
    let tray = view(&state.snapshot(), &monitor, 1000);
    assert!(tray.claude_status.contains("not checked"));
    assert!(!tray.claude_status.contains("0%"));
    assert_eq!(tray.pause_label, "Pause monitoring");
    assert_eq!(
        pause_action(&monitor),
        Some(MonitorSettings {
            paused: true,
            interval_minutes: 5
        })
    );
}
#[test]
fn reported_reset_passed_never_claims_replenished_quota() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(Storage::open(&dir.path().join("db")), Arc::new(Empty));
    let mut snapshot = state.snapshot();
    snapshot.agents.push(AgentState {
        agent_id: AgentId::Codex,
        version: None,
        availability: Availability::Detected,
        auth_state: AuthState::Ready,
        capabilities: Capabilities::default(),
        observed_ms: 1000,
        diagnostic_code: None,
        account_usage: None,
        quota: Some(QuotaSnapshot {
            source: QuotaSource::CodexAppServer,
            observed_ms: 1000,
            diagnostic_code: None,
            windows: vec![jentrics_core::adapters::codex::QuotaWindow {
                bucket_id: "private arbitrary id".into(),
                window_minutes: Some(10080),
                used_percent: Some(80.0),
                resets_at_ms: Some(2000),
                observed_ms: 1000,
            }],
        }),
    });
    let before = view(&snapshot, &state.monitor_settings(), 1000);
    assert!(before.codex_reset.contains("reported"));
    let after = view(&snapshot, &state.monitor_settings(), 2001);
    assert!(after.codex_reset.contains("awaiting fresh"));
    assert!(!after.codex_reset.contains("available"));
    assert!(!after.tooltip.contains("private"));
}
