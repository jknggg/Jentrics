use jentrics_core::{
    model::*,
    state::{AppState, Collector},
    storage::{Storage, StorageError},
};
use std::{future::Future, pin::Pin, sync::Arc};
struct Fixture(bool);
impl Collector for Fixture {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async {
            if self.0 {
                return Err(DiagnosticCode::TemporaryDirectoryUnavailable);
            }
            let at = jentrics_core::state::now_ms();
            Ok(vec![serde_json::from_value(serde_json::json!({"agentId":"codex","version":"0.165.0-alpha.1.2","availability":"detected","authState":"ready","capabilities":{"discovery":true,"authReadiness":true,"quotas":true,"usage":false,"sessionStart":false},"observedMs":at,"diagnosticCode":null,"quota":{"source":"codexAppServer","observedMs":at,"diagnosticCode":null,"windows":[{"bucketId":"codex:primary","windowMinutes":10080,"usedPercent":14,"resetsAtMs":null,"observedMs":at}]}})).unwrap()])
        })
    }
}
#[tokio::test]
async fn refresh_without_a_dashboard_records_history_and_confirmed_clear_preserves_latest_metadata()
{
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let state = AppState::new(Storage::open(&path), Arc::new(Fixture(false)));
    let snapshot = state.refresh().await;
    let history = state.history(24).unwrap();
    assert_eq!(history.samples.len(), 1);
    assert_eq!(history.samples[0].used_percent, Some(14.0));
    assert!(state.clear_history(false).is_err());
    assert_eq!(state.history(24).unwrap().samples.len(), 1);
    state.clear_history(true).unwrap();
    assert!(state.history(24).unwrap().samples.is_empty());
    assert_eq!(
        Storage::open(&path).unwrap().load_agents().unwrap(),
        snapshot.agents
    );
    assert_eq!(state.set_history_retention(30).unwrap().retention_days, 30);
    assert_eq!(state.history_settings().unwrap().retention_days, 30);
}
#[tokio::test]
async fn collection_wide_failures_are_recorded_as_error_checks_without_fabricated_quota() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(
        Storage::open(&dir.path().join("state.sqlite")),
        Arc::new(Fixture(true)),
    );
    state.refresh().await;
    let history = state.history(24).unwrap();
    assert!(history.samples.is_empty());
    assert_eq!(history.checks.len(), 2);
    assert!(history.checks.iter().all(|check| check.status == "error"
        && check.diagnostic_code == Some(DiagnosticCode::TemporaryDirectoryUnavailable)));
}
#[tokio::test]
async fn history_errors_and_invalid_arguments_never_become_an_empty_success() {
    let unavailable = AppState::new(Err(StorageError::Unavailable), Arc::new(Fixture(false)));
    assert!(unavailable.history(24).is_err());
    assert!(unavailable.history_settings().is_err());
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(
        Storage::open(&dir.path().join("state.sqlite")),
        Arc::new(Fixture(false)),
    );
    assert!(state.history(25).is_err());
    assert!(state.set_history_retention(0).is_err());
}
