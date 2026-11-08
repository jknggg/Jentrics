use jentrics_core::{
    model::*,
    state::{AppState, Collector},
    storage::{Storage, StorageError},
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

struct CountingCollector {
    calls: AtomicUsize,
}

struct FailedCollector;

#[tokio::test]
async fn successful_save_clears_a_transient_storage_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let state = AppState::new(
        Storage::open(&path),
        Arc::new(CountingCollector {
            calls: AtomicUsize::new(0),
        }),
    );
    let blocker = rusqlite::Connection::open(&path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert_eq!(
        state.refresh().await.storage_error,
        Some(DiagnosticCode::StorageUnavailable)
    );
    blocker.execute_batch("ROLLBACK").unwrap();
    assert_eq!(state.refresh().await.storage_error, None);
    assert_eq!(
        Storage::open(&path).unwrap().load_agents().unwrap().len(),
        1
    );
}
impl Collector for FailedCollector {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async { Err(DiagnosticCode::TemporaryDirectoryUnavailable) })
    }
}
#[tokio::test]
async fn first_collection_error_is_visible_without_saved_agents() {
    let state = AppState::new(Err(StorageError::Unavailable), Arc::new(FailedCollector));
    let snapshot = state.refresh().await;
    assert_eq!(
        snapshot.collection_error,
        Some(DiagnosticCode::TemporaryDirectoryUnavailable)
    );
    assert!(!snapshot.refreshing);
}
#[tokio::test]
async fn quit_cancels_collection_and_rejects_new_jobs() {
    let collector = Arc::new(CountingCollector {
        calls: AtomicUsize::new(0),
    });
    let state = AppState::new(Err(StorageError::Unavailable), collector.clone());
    let subscribed = state.clone();
    let task = tokio::spawn(async move { subscribed.refresh().await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    state.stop().await;
    task.await.unwrap();
    let result = state.refresh().await;
    assert!(!result.refreshing);
    assert_eq!(collector.calls.load(Ordering::SeqCst), 1);
    assert!(result.agents.is_empty());
}
impl Collector for CountingCollector {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(80)).await;
            Ok(vec![AgentState {
                agent_id: AgentId::Codex,
                version: Some("0.162.0-alpha.17.2".into()),
                availability: Availability::Detected,
                auth_state: AuthState::Ready,
                capabilities: Capabilities::default(),
                observed_ms: 1000,
                diagnostic_code: None,
                quota: None,
                account_usage: None,
            }])
        })
    }
}
#[tokio::test]
async fn concurrent_refreshes_share_one_collection_and_result() {
    let dir = tempfile::tempdir().unwrap();
    let collector = Arc::new(CountingCollector {
        calls: AtomicUsize::new(0),
    });
    let state = AppState::new(
        Storage::open(&dir.path().join("state.sqlite")),
        collector.clone(),
    );
    let (first, second) = tokio::join!(state.refresh(), state.refresh());
    assert_eq!(collector.calls.load(Ordering::SeqCst), 1);
    assert_eq!(first, second);
    assert_eq!(first.agents.len(), 1);
    assert!(!first.refreshing);
}
#[tokio::test]
async fn storage_failure_keeps_latest_state_visible() {
    let state = AppState::new(
        Err(StorageError::Unavailable),
        Arc::new(CountingCollector {
            calls: AtomicUsize::new(0),
        }),
    );
    let result = state.refresh().await;
    assert_eq!(result.agents.len(), 1);
    assert_eq!(
        result.storage_error,
        Some(DiagnosticCode::StorageUnavailable)
    );
}
#[tokio::test]
async fn dropping_window_subscriber_does_not_cancel_core_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let collector = Arc::new(CountingCollector {
        calls: AtomicUsize::new(0),
    });
    let state = AppState::new(
        Storage::open(&dir.path().join("state.sqlite")),
        collector.clone(),
    );
    let subscribed = state.clone();
    let task = tokio::spawn(async move { subscribed.refresh().await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    task.abort();
    tokio::time::sleep(Duration::from_millis(160)).await;
    assert_eq!(state.snapshot().agents.len(), 1);
    assert!(!state.snapshot().refreshing);
    assert_eq!(
        Storage::open(&dir.path().join("state.sqlite"))
            .unwrap()
            .load_agents()
            .unwrap()
            .len(),
        1
    );
}
