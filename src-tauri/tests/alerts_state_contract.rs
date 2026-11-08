use jentrics_core::{
    alerts::AlertSettings,
    model::*,
    state::{AlertPermission, AppState, Collector},
    storage::Storage,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
struct Quota;
impl Collector for Quota {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async {
            let now = jentrics_core::state::now_ms();
            Ok(vec![serde_json::from_value(serde_json::json!({"agentId":"codex","version":"0.165.0-alpha.1.2","availability":"detected","authState":"ready","capabilities":{"discovery":true,"authReadiness":true,"quotas":true,"usage":false,"sessionStart":false},"observedMs":now,"diagnosticCode":null,"quota":{"source":"codexAppServer","observedMs":now,"diagnosticCode":null,"windows":[{"bucketId":"primary","windowMinutes":300,"usedPercent":96,"resetsAtMs":now/86400000*86400000+86400000,"observedMs":now}]}})).unwrap()])
        })
    }
}
#[tokio::test]
async fn alerts_are_opt_in_and_successful_delivery_dedup_persists_across_restart() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("state.sqlite");
    let received = Arc::new(Mutex::new(Vec::new()));
    let state = AppState::new(Storage::open(&path), Arc::new(Quota));
    let sink = received.clone();
    state.set_alert_sink(
        Arc::new(move |event| {
            sink.lock().unwrap().push(event.key.clone());
            Ok(())
        }),
        AlertPermission::Granted,
    );
    state.refresh().await;
    assert!(received.lock().unwrap().is_empty());
    let settings = AlertSettings {
        enabled: true,
        near_reset: false,
        expected_reset: false,
        ..Default::default()
    };
    state.save_alert_settings(settings).unwrap();
    state.refresh().await;
    assert_eq!(received.lock().unwrap().len(), 1);
    state.refresh().await;
    assert_eq!(received.lock().unwrap().len(), 1);
    state.stop().await;
    drop(state);
    let state = AppState::new(Storage::open(&path), Arc::new(Quota));
    let sink = received.clone();
    state.set_alert_sink(
        Arc::new(move |event| {
            sink.lock().unwrap().push(event.key.clone());
            Ok(())
        }),
        AlertPermission::Granted,
    );
    state.refresh().await;
    assert_eq!(received.lock().unwrap().len(), 1);
    state.stop().await;
}
struct Broken;
impl Collector for Broken {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async { Err(DiagnosticCode::TemporaryDirectoryUnavailable) })
    }
}
#[tokio::test]
async fn global_collection_error_notifies_once_without_fabricating_a_provider() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let state = AppState::new(
        Storage::open(&dir.path().join("state.sqlite")),
        Arc::new(Broken),
    );
    let received = Arc::new(Mutex::new(Vec::new()));
    let sink = received.clone();
    state.set_alert_sink(
        Arc::new(move |event| {
            sink.lock().unwrap().push(event.key.clone());
            Ok(())
        }),
        AlertPermission::Granted,
    );
    state
        .save_alert_settings(AlertSettings {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    state.refresh().await;
    state.refresh().await;
    {
        let keys = received.lock().unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].agent_id, None);
    }
    state.stop().await;
}
