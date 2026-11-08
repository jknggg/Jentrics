use jentrics_core::{
    model::*,
    state::{AppState, Collector, MonitorSettings},
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

struct Sources(AtomicUsize);
impl Collector for Sources {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async move {
            let call = self.0.fetch_add(1, Ordering::SeqCst);
            Ok([AgentId::Claude, AgentId::Codex]
                .into_iter()
                .map(|agent_id| AgentState {
                    agent_id,
                    availability: if call == 0 {
                        Availability::NotFound
                    } else {
                        Availability::Detected
                    },
                    version: None,
                    auth_state: AuthState::Unknown,
                    capabilities: Capabilities::default(),
                    observed_ms: 1000,
                    diagnostic_code: None,
                    quota: None,
                    account_usage: None,
                })
                .collect())
        })
    }
}
#[tokio::test]
async fn no_sources_suspend_and_manual_discovery_recovers_without_duplicate_probe() {
    let dir = tempfile::tempdir().unwrap();
    let collector = Arc::new(Sources(AtomicUsize::new(0)));
    let state = AppState::new(
        Storage::open(&dir.path().join("state.sqlite")),
        collector.clone(),
    );
    let runner = tokio::spawn(state.clone().run_monitor());
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.monitor_settings().runtime_reason.is_none() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(state.monitor_settings()).unwrap()["runtimeReason"],
        "noQualifiedProviders"
    );
    assert_eq!(state.monitor_settings().next_check_ms, None);
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert_eq!(collector.0.load(Ordering::SeqCst), 1);
    state.refresh().await;
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert_eq!(collector.0.load(Ordering::SeqCst), 2);
    assert!(state.monitor_settings().runtime_reason.is_none());
    assert!(state.monitor_settings().next_check_ms.is_some());
    state
        .save_monitor_settings(MonitorSettings {
            paused: true,
            interval_minutes: 5,
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(state.monitor_settings()).unwrap()["runtimeReason"],
        "paused"
    );
    state
        .save_monitor_settings(MonitorSettings {
            paused: false,
            interval_minutes: 5,
        })
        .unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert_eq!(collector.0.load(Ordering::SeqCst), 3);
    state.stop().await;
    runner.await.unwrap();
}

#[test]
fn settings_navigation_is_one_shot() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(
        Storage::open(&dir.path().join("state.sqlite")),
        Arc::new(Sources(AtomicUsize::new(0))),
    );
    assert!(!state.take_settings_request());
    state.request_settings();
    assert!(state.take_settings_request());
    assert!(!state.take_settings_request());
}

struct FixedSources {
    calls: AtomicUsize,
    rows: Vec<AgentState>,
}
impl Collector for FixedSources {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.rows.clone())
        })
    }
}
#[tokio::test]
async fn supported_failed_probes_back_off_instead_of_suspending() {
    let source = Sources(AtomicUsize::new(1));
    let mut rows = source.collect().await.unwrap();
    for row in &mut rows {
        row.availability = Availability::ProbeFailed;
        row.diagnostic_code = Some(DiagnosticCode::ProbeTimeout);
    }
    let collector = Arc::new(FixedSources {
        calls: AtomicUsize::new(0),
        rows,
    });
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(Storage::open(&dir.path().join("db")), collector.clone());
    let runner = tokio::spawn(state.clone().run_monitor());
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.monitor_settings().next_check_ms.is_none() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        state.monitor_settings().effective_interval_minutes,
        Some(10)
    );
    assert!(state.monitor_settings().runtime_reason.is_none());
    state.stop().await;
    runner.await.unwrap();
    assert_eq!(collector.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn explicit_resume_rechecks_unsupported_sources_once_and_suspends_again() {
    let source = Sources(AtomicUsize::new(0));
    let mut rows = source.collect().await.unwrap();
    for row in &mut rows {
        row.availability = Availability::Unsupported;
    }
    let collector = Arc::new(FixedSources {
        calls: AtomicUsize::new(0),
        rows,
    });
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(Storage::open(&dir.path().join("db")), collector.clone());
    let runner = tokio::spawn(state.clone().run_monitor());
    for count in 1..=2 {
        tokio::time::timeout(Duration::from_secs(2), async {
            while collector.calls.load(Ordering::SeqCst) < count
                || state.monitor_settings().runtime_reason.is_none()
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(collector.calls.load(Ordering::SeqCst), count);
        assert!(state.monitor_settings().next_check_ms.is_none());
        if count == 1 {
            state
                .save_monitor_settings(MonitorSettings {
                    paused: false,
                    interval_minutes: 5,
                })
                .unwrap();
        }
    }
    state.stop().await;
    runner.await.unwrap();
}
