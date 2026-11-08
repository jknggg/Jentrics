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
use tokio::sync::{mpsc, Semaphore};

const PERIOD: Duration = Duration::from_millis(100);
struct TestCollector {
    calls: AtomicUsize,
    started: mpsc::UnboundedSender<usize>,
    gate: Option<Arc<Semaphore>>,
    fail_first: bool,
}
impl Collector for TestCollector {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            self.started.send(call).unwrap();
            if let Some(gate) = &self.gate {
                gate.acquire().await.unwrap().forget();
            }
            if self.fail_first && call == 1 {
                return Err(DiagnosticCode::TemporaryDirectoryUnavailable);
            }
            Ok(vec![AgentState {
                agent_id: AgentId::Codex,
                version: Some("0.162.1".into()),
                availability: Availability::Detected,
                auth_state: AuthState::Ready,
                capabilities: Capabilities {
                    quotas: true,
                    ..Default::default()
                },
                observed_ms: call as i64 * 1000,
                diagnostic_code: None,
                account_usage: None,
                quota: Some(QuotaSnapshot {
                    source: QuotaSource::CodexAppServer,
                    observed_ms: call as i64 * 1000,
                    diagnostic_code: None,
                    windows: vec![jentrics_core::adapters::codex::QuotaWindow {
                        bucket_id: "codex:primary".into(),
                        window_minutes: Some(10080),
                        used_percent: Some(call as f64),
                        resets_at_ms: None,
                        observed_ms: call as i64 * 1000,
                    }],
                }),
            }])
        })
    }
}
fn setup(
    storage: Result<Storage, StorageError>,
    fail_first: bool,
    gate: Option<Arc<Semaphore>>,
) -> (
    Arc<AppState>,
    Arc<TestCollector>,
    mpsc::UnboundedReceiver<usize>,
    mpsc::UnboundedReceiver<DashboardState>,
) {
    let (started_tx, started) = mpsc::unbounded_channel();
    let (snapshots_tx, snapshots) = mpsc::unbounded_channel();
    let collector = Arc::new(TestCollector {
        calls: AtomicUsize::new(0),
        started: started_tx,
        gate,
        fail_first,
    });
    let state = AppState::new(storage, collector.clone());
    state.set_publisher(Arc::new(move |snapshot| {
        snapshots_tx.send(snapshot.clone()).unwrap();
    }));
    (state, collector, started, snapshots)
}
async fn completed(snapshots: &mut mpsc::UnboundedReceiver<DashboardState>) -> DashboardState {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = snapshots.recv().await.unwrap();
            if !snapshot.refreshing {
                return snapshot;
            }
        }
    })
    .await
    .expect("automatic refresh did not complete")
}
#[tokio::test]
async fn tray_monitor_refreshes_on_startup_and_periodically_persists_without_any_dashboard() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let (state, _, mut started, mut snapshots) = setup(Storage::open(&path), false, None);
    let monitor = tokio::spawn(state.clone().run_periodic_refresh(PERIOD));
    let first = completed(&mut snapshots).await;
    assert_eq!(
        first.agents[0].quota.as_ref().unwrap().windows[0].used_percent,
        Some(1.0)
    );
    assert_eq!(started.recv().await, Some(1));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), started.recv())
            .await
            .is_err()
    );
    let second = completed(&mut snapshots).await;
    assert_eq!(
        second.agents[0].quota.as_ref().unwrap().windows[0].used_percent,
        Some(2.0)
    );
    assert_eq!(
        Storage::open(&path).unwrap().load_agents().unwrap(),
        second.agents
    );
    state.stop().await;
    tokio::time::timeout(Duration::from_secs(1), monitor)
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn failed_automatic_check_remains_visible_and_recovers_at_the_next_period() {
    let (state, _, _started, mut snapshots) = setup(Err(StorageError::Unavailable), true, None);
    let monitor = tokio::spawn(state.clone().run_periodic_refresh(PERIOD));
    assert_eq!(
        completed(&mut snapshots).await.collection_error,
        Some(DiagnosticCode::TemporaryDirectoryUnavailable)
    );
    let next = completed(&mut snapshots).await;
    assert!(next.collection_error.is_none());
    assert_eq!(next.agents.len(), 1);
    state.stop().await;
    monitor.await.unwrap();
}
#[tokio::test]
async fn manual_refresh_joins_running_automatic_check_and_quit_cancels_next_collection() {
    let gate = Arc::new(Semaphore::new(0));
    let (state, collector, mut started, mut snapshots) =
        setup(Err(StorageError::Unavailable), false, Some(gate.clone()));
    let monitor = tokio::spawn(state.clone().run_periodic_refresh(PERIOD));
    assert_eq!(started.recv().await, Some(1));
    let manual = state.refresh();
    tokio::pin!(manual);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), manual.as_mut())
            .await
            .is_err()
    );
    gate.add_permits(1);
    let automatic = completed(&mut snapshots).await;
    assert_eq!(manual.await, automatic);
    assert_eq!(collector.calls.load(Ordering::SeqCst), 1);
    assert_eq!(started.recv().await, Some(2));
    state.stop().await;
    tokio::time::timeout(Duration::from_secs(1), monitor)
        .await
        .unwrap()
        .unwrap();
    assert!(!state.snapshot().refreshing);
    assert_eq!(collector.calls.load(Ordering::SeqCst), 2);
    tokio::time::sleep(PERIOD * 2).await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 2);
}
#[tokio::test]
async fn delayed_timer_does_not_replay_missed_checks_and_idle_quit_stops_future_work() {
    let (state, collector, mut started, mut snapshots) =
        setup(Err(StorageError::Unavailable), false, None);
    let monitor = tokio::spawn(state.clone().run_periodic_refresh(PERIOD));
    completed(&mut snapshots).await;
    assert_eq!(started.recv().await, Some(1));
    // Simulate the process not being scheduled for several polling periods.
    std::thread::sleep(PERIOD * 4);
    completed(&mut snapshots).await;
    assert_eq!(started.recv().await, Some(2));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), started.recv())
            .await
            .is_err()
    );
    state.stop().await;
    tokio::time::timeout(Duration::from_secs(1), monitor)
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(PERIOD * 2).await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 2);
}
