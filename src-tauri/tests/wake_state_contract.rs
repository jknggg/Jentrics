use jentrics_core::{
    model::*,
    state::{AppState, Collector},
    storage::StorageError,
    wake::{PreparedWake, QuotaEffect, WakeReason, WakeRunner, WakeStatus},
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::sync::{mpsc, Semaphore};

struct Fixture {
    timeout: AtomicBool,
    runs: AtomicUsize,
    mode: AtomicUsize,
    gate: Option<Arc<Semaphore>>,
    started: mpsc::UnboundedSender<()>,
}
impl Collector for Fixture {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async {
            let now = jentrics_core::state::now_ms();
            let mode = self.mode.load(Ordering::SeqCst);
            if mode == 3 && self.runs.load(Ordering::SeqCst) > 0 {
                return Err(DiagnosticCode::AuthUnavailable);
            }
            let minutes = if mode == 2 { 10080 } else { 300 };
            let reset = if mode == 1 || self.runs.load(Ordering::SeqCst) > 0 {
                now + 3600000
            } else {
                now - 1000
            };
            let agent:AgentState=serde_json::from_value(serde_json::json!({"agentId":"codex","version":"0.165.0-alpha.1.2","availability":"detected","authState":"ready","capabilities":{"discovery":true,"authReadiness":true,"quotas":true,"usage":false,"sessionStart":false},"observedMs":now,"diagnosticCode":null,"quota":{"source":"codexAppServer","observedMs":now,"diagnosticCode":null,"windows":[{"bucketId":"codex:primary","windowMinutes":minutes,"usedPercent":0,"resetsAtMs":reset,"observedMs":now}]}})).unwrap();
            Ok(vec![agent])
        })
    }
}
impl WakeRunner for Fixture {
    fn prepare(
        &self,
        agent_id: AgentId,
    ) -> Pin<Box<dyn Future<Output = Result<PreparedWake, WakeReason>> + Send + '_>> {
        Box::pin(async move {
            Ok(PreparedWake {
                agent_id,
                executable: std::env::current_exe().unwrap(),
                version: "0.165.0-alpha.1.2".into(),
            })
        })
    }
    fn run(
        &self,
        _: PreparedWake,
    ) -> Pin<Box<dyn Future<Output = Result<(), WakeReason>> + Send + '_>> {
        Box::pin(async {
            self.runs.fetch_add(1, Ordering::SeqCst);
            self.started.send(()).unwrap();
            if let Some(gate) = &self.gate {
                gate.acquire().await.unwrap().forget();
            }
            if self.timeout.load(Ordering::SeqCst) {
                return Err(WakeReason::TimedOut);
            }
            Ok(())
        })
    }
}
fn setup(
    gate: Option<Arc<Semaphore>>,
) -> (Arc<AppState>, Arc<Fixture>, mpsc::UnboundedReceiver<()>) {
    let (started, receiver) = mpsc::unbounded_channel();
    let fixture = Arc::new(Fixture {
        timeout: AtomicBool::new(false),
        runs: AtomicUsize::new(0),
        mode: AtomicUsize::new(0),
        gate,
        started,
    });
    let state = AppState::new(Err(StorageError::Unavailable), fixture.clone());
    state.set_wake_runner(fixture.clone());
    (state, fixture, receiver)
}
#[tokio::test]
async fn prepare_and_cancel_never_send_a_model_request() {
    let (state, fixture, _receiver) = setup(None);
    let offer = state.prepare_wake(AgentId::Codex).await;
    assert!(offer.enabled);
    let result = state
        .confirm_wake(AgentId::Codex, offer.confirmation_id.unwrap(), false)
        .await;
    assert_eq!(result.status, WakeStatus::Cancelled);
    assert_eq!(fixture.runs.load(Ordering::SeqCst), 0);
    let result = state
        .confirm_wake(AgentId::Codex, "unprepared".into(), true)
        .await;
    assert_eq!(result.reason, Some(WakeReason::PrepareRequired));
    assert_eq!(fixture.runs.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn prepare_disables_active_and_weekly_only_accounts() {
    let (state, fixture, _receiver) = setup(None);
    for (mode, expected) in [(1, WakeReason::ActiveWindow), (2, WakeReason::WeeklyOnly)] {
        fixture.mode.store(mode, Ordering::SeqCst);
        let offer = state.prepare_wake(AgentId::Codex).await;
        assert!(!offer.enabled);
        assert_eq!(offer.reason, Some(expected));
    }
    assert_eq!(fixture.runs.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn confirm_rechecks_eligibility_and_prevents_cross_provider_or_replayed_consent() {
    let (state, fixture, _receiver) = setup(None);
    let offer = state.prepare_wake(AgentId::Codex).await;
    let wrong = state
        .confirm_wake(
            AgentId::Claude,
            offer.confirmation_id.clone().unwrap(),
            true,
        )
        .await;
    assert_eq!(wrong.reason, Some(WakeReason::PrepareRequired));
    fixture.mode.store(1, Ordering::SeqCst);
    let changed = state
        .confirm_wake(AgentId::Codex, offer.confirmation_id.unwrap(), true)
        .await;
    assert_eq!(changed.reason, Some(WakeReason::ActiveWindow));
    assert_eq!(fixture.runs.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn completed_request_reports_observation_and_cooldown_and_never_runs_twice() {
    let (state, fixture, _receiver) = setup(None);
    let offer = state.prepare_wake(AgentId::Codex).await;
    let id = offer.confirmation_id.unwrap();
    let result = state.confirm_wake(AgentId::Codex, id.clone(), true).await;
    assert_eq!(result.status, WakeStatus::Completed);
    assert_eq!(result.quota_effect, QuotaEffect::NewWindowObserved);
    let repeated = state.confirm_wake(AgentId::Codex, id, true).await;
    assert_eq!(repeated.reason, Some(WakeReason::Cooldown));
    assert_eq!(fixture.runs.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn quit_cancels_inflight_wake_and_prevents_another_action() {
    let (state, fixture, mut started) = setup(Some(Arc::new(Semaphore::new(0))));
    let offer = state.prepare_wake(AgentId::Codex).await;
    let id = offer.confirmation_id.unwrap();
    let task_state = state.clone();
    let task = tokio::spawn(async move { task_state.confirm_wake(AgentId::Codex, id, true).await });
    tokio::time::timeout(std::time::Duration::from_secs(2), started.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        state.prepare_wake(AgentId::Codex).await.reason,
        Some(WakeReason::Busy)
    );
    state.stop().await;
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.status, WakeStatus::Cancelled);
    assert_eq!(
        state.prepare_wake(AgentId::Codex).await.reason,
        Some(WakeReason::Stopped)
    );
    assert_eq!(fixture.runs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn timeout_does_not_claim_a_confirmed_window_or_retry_the_request() {
    let (state, fixture, _receiver) = setup(None);
    fixture.timeout.store(true, Ordering::SeqCst);
    let offer = state.prepare_wake(AgentId::Codex).await;
    let result = state
        .confirm_wake(AgentId::Codex, offer.confirmation_id.unwrap(), true)
        .await;
    assert_eq!(result.status, WakeStatus::TimedOut);
    assert_eq!(result.quota_effect, QuotaEffect::Unverified);
    assert_eq!(fixture.runs.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn successful_request_with_failed_quota_reread_remains_unverified() {
    let (state, fixture, _receiver) = setup(None);
    fixture.mode.store(3, Ordering::SeqCst);
    let offer = state.prepare_wake(AgentId::Codex).await;
    let result = state
        .confirm_wake(AgentId::Codex, offer.confirmation_id.unwrap(), true)
        .await;
    assert_eq!(result.status, WakeStatus::Completed);
    assert_eq!(result.quota_effect, QuotaEffect::Unverified);
    assert_eq!(
        result.dashboard.collection_error,
        Some(DiagnosticCode::AuthUnavailable)
    );
}
#[tokio::test]
async fn automatic_monitor_never_runs_the_configured_model_runner() {
    let (state, fixture, _receiver) = setup(None);
    let monitor = tokio::spawn(
        state
            .clone()
            .run_periodic_refresh(std::time::Duration::from_millis(10)),
    );
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    state.stop().await;
    monitor.await.unwrap();
    assert_eq!(fixture.runs.load(Ordering::SeqCst), 0);
}
