//! Production-loop overdue-timer coverage, not physical OS sleep/resume proof.
use jentrics_core::{
    model::{AgentId, AgentState, AuthState, Availability, Capabilities, DiagnosticCode},
    state::{AppState, Collector},
    storage::Storage,
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
use tokio::sync::Semaphore;

struct MetadataCollector {
    calls: AtomicUsize,
    gate: Option<Arc<Semaphore>>,
}
impl Collector for MetadataCollector {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(gate) = &self.gate {
                gate.acquire().await.unwrap().forget();
            }
            Ok(vec![AgentState {
                agent_id: AgentId::Codex,
                version: Some("0.162.1".into()),
                availability: Availability::Detected,
                auth_state: AuthState::Ready,
                capabilities: Capabilities {
                    discovery: true,
                    ..Default::default()
                },
                observed_ms: call as i64 * 1000,
                diagnostic_code: None,
                quota: None,
                account_usage: None,
            }])
        })
    }
}

// Tokio documents that advance() does not guarantee ready timers have run.
// Keep this task runnable while yielding, so auto-advance cannot add time.
async fn dispatch_ready_work() {
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn production_overdue_timer_runs_one_fresh_check_then_waits_a_full_period() {
    let directory = tempfile::tempdir().unwrap();
    let collector = Arc::new(MetadataCollector {
        calls: AtomicUsize::new(0),
        gate: None,
    });
    let state = AppState::new(
        Storage::open(&directory.path().join("state.sqlite")),
        collector.clone(),
    );
    let task = tokio::spawn(state.clone().run_monitor());
    dispatch_ready_work().await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 1);
    assert_eq!(state.monitor_settings().effective_interval_minutes, Some(5));

    // Six periods become overdue together; no real machine clock is changed.
    tokio::time::advance(Duration::from_secs(30 * 60)).await;
    dispatch_ready_work().await;
    assert_eq!(
        collector.calls.load(Ordering::SeqCst),
        2,
        "Missed checks replayed"
    );
    tokio::time::advance(Duration::from_secs(299)).await;
    dispatch_ready_work().await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 2);
    tokio::time::advance(Duration::from_secs(1)).await;
    dispatch_ready_work().await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 3);

    state.stop().await;
    task.await.unwrap();
    tokio::time::advance(Duration::from_secs(3600)).await;
    dispatch_ready_work().await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 3);
}

#[tokio::test(start_paused = true)]
async fn production_inflight_check_is_shared_and_next_period_starts_after_completion() {
    let directory = tempfile::tempdir().unwrap();
    let gate = Arc::new(Semaphore::new(0));
    let collector = Arc::new(MetadataCollector {
        calls: AtomicUsize::new(0),
        gate: Some(gate.clone()),
    });
    let state = AppState::new(
        Storage::open(&directory.path().join("state.sqlite")),
        collector.clone(),
    );
    let task = tokio::spawn(state.clone().run_monitor());
    dispatch_ready_work().await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 1);
    assert!(state.snapshot().refreshing);

    tokio::time::advance(Duration::from_secs(30 * 60)).await;
    dispatch_ready_work().await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 1);
    let manual_state = state.clone();
    let manual = tokio::spawn(async move { manual_state.refresh().await });
    dispatch_ready_work().await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 1);
    gate.add_permits(1);
    dispatch_ready_work().await;
    assert_eq!(manual.await.unwrap().agents[0].observed_ms, 1000);
    assert_eq!(collector.calls.load(Ordering::SeqCst), 1);
    assert!(!state.snapshot().refreshing);

    tokio::time::advance(Duration::from_secs(299)).await;
    dispatch_ready_work().await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    dispatch_ready_work().await;
    assert_eq!(collector.calls.load(Ordering::SeqCst), 2);
    assert!(state.snapshot().refreshing);
    state.stop().await;
    task.await.unwrap();
    assert!(!state.snapshot().refreshing);
    assert_eq!(collector.calls.load(Ordering::SeqCst), 2);
}
