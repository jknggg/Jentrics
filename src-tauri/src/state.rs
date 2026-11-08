mod alerts;
mod claude;
mod configuration;
mod diagnostics;
mod history;
mod metrics;
mod monitor;
pub use alerts::{AlertError, AlertPermission, AlertSettingsView};
pub use metrics::{MetricStatus, MetricView};
mod wake;
use crate::{
    model::{AgentState, DashboardState, DiagnosticCode},
    storage::{Storage, StorageError},
};
pub use monitor::AUTO_REFRESH_INTERVAL;
pub use monitor::{MonitorError, MonitorRuntimeReason, MonitorSettings, MonitorSettingsView};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

pub trait Collector: Send + Sync {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>>;
}
type Publisher = Arc<dyn Fn(&DashboardState) + Send + Sync>;
pub struct AppState {
    metrics: metrics::MetricControl,
    monitor: monitor::MonitorControl,
    alerts: alerts::AlertControl,
    claude_bridge: Mutex<Option<Arc<crate::claude_bridge::control::BridgeControl>>>,
    wake: wake::WakeControl,
    snapshot: Mutex<DashboardState>,
    storage: Mutex<Option<Storage>>,
    collector: Arc<dyn Collector>,
    in_flight: Mutex<Option<watch::Receiver<Option<DashboardState>>>>,
    shutdown: watch::Sender<bool>,
    publisher: Mutex<Option<Publisher>>,
    settings_requested: std::sync::atomic::AtomicBool,
    diagnostics: Mutex<Option<Arc<crate::diagnostics::Diagnostics>>>,
}
impl AppState {
    pub fn new(storage: Result<Storage, StorageError>, collector: Arc<dyn Collector>) -> Arc<Self> {
        let (storage, agents, storage_error) = match storage {
            Ok(storage) => match storage.load_agents() {
                Ok(agents) => (Some(storage), agents, None),
                Err(error) => (None, vec![], Some(storage_code(error))),
            },
            Err(error) => (None, vec![], Some(storage_code(error))),
        };
        let (shutdown, _) = watch::channel(false);
        Arc::new(Self {
            metrics: metrics::MetricControl::default(),
            monitor: monitor::MonitorControl::default(),
            alerts: alerts::AlertControl::default(),
            claude_bridge: Mutex::new(None),
            wake: wake::WakeControl::default(),
            snapshot: Mutex::new(DashboardState {
                schema_version: 1,
                agents,
                refreshing: false,
                last_refresh_ms: None,
                storage_error,
                collection_error: None,
            }),
            storage: Mutex::new(storage),
            collector,
            in_flight: Mutex::new(None),
            shutdown,
            publisher: Mutex::new(None),
            settings_requested: std::sync::atomic::AtomicBool::new(false),
            diagnostics: Mutex::new(None),
        })
    }
    pub fn snapshot(&self) -> DashboardState {
        self.snapshot.lock().expect("state lock poisoned").clone()
    }
    pub fn request_settings(&self) {
        self.settings_requested
            .store(true, std::sync::atomic::Ordering::Release);
    }
    pub fn take_settings_request(&self) -> bool {
        self.settings_requested
            .swap(false, std::sync::atomic::Ordering::AcqRel)
    }
    pub fn set_publisher(&self, publisher: Publisher) {
        *self.publisher.lock().expect("publisher lock poisoned") = Some(publisher);
    }
    fn publish(&self) {
        let publisher = self
            .publisher
            .lock()
            .expect("publisher lock poisoned")
            .clone();
        if let Some(publisher) = publisher {
            publisher(&self.snapshot());
        }
    }
    pub async fn refresh(self: &Arc<Self>) -> DashboardState {
        let mut receiver = {
            let mut gate = self.in_flight.lock().expect("refresh lock poisoned");
            if *self.shutdown.borrow() {
                return self.snapshot();
            }
            match gate.as_ref() {
                Some(receiver) => receiver.clone(),
                None => {
                    let (sender, receiver) = watch::channel(None);
                    *gate = Some(receiver.clone());
                    self.snapshot
                        .lock()
                        .expect("state lock poisoned")
                        .refreshing = true;
                    let state = self.clone();
                    tokio::spawn(async move {
                        let started = std::time::Instant::now();
                        state.publish();
                        let mut shutdown = state.shutdown.subscribe();
                        let result = if *shutdown.borrow() {
                            None
                        } else {
                            tokio::select! { result=state.collector.collect()=>Some(result), _=shutdown.changed()=>None }
                        };
                        let previous = state.snapshot();
                        if let Some(result) = result {
                            match result {
                                Ok(mut agents) => {
                                    let mut storage =
                                        state.storage.lock().expect("storage lock poisoned");
                                    state.attach_claude_capture(&mut agents);
                                    let error = storage
                                        .as_mut()
                                        .map(|storage| storage.save_agents(&agents))
                                        .transpose()
                                        .err()
                                        .map(storage_code);
                                    let mut snapshot =
                                        state.snapshot.lock().expect("state lock poisoned");
                                    snapshot.last_refresh_ms = Some(now_ms());
                                    snapshot.agents = agents;
                                    snapshot.collection_error = None;
                                    if storage.is_some() {
                                        snapshot.storage_error = error;
                                    }
                                }
                                Err(error) => {
                                    // Record the failed check without inventing quota samples.
                                    // These are the two providers managed by DiscoveryCollector.
                                    let history_result = state
                                        .storage
                                        .lock()
                                        .expect("storage lock poisoned")
                                        .as_mut()
                                        .map(|storage| {
                                            storage.record_history_failure(
                                                &[
                                                    crate::model::AgentId::Claude,
                                                    crate::model::AgentId::Codex,
                                                ],
                                                now_ms(),
                                                error,
                                            )
                                        });
                                    // A collection-wide failure keeps saved rows, visibly unverified.
                                    let mut snapshot =
                                        state.snapshot.lock().expect("state lock poisoned");
                                    for agent in &mut snapshot.agents {
                                        agent.auth_state = crate::model::AuthState::Unknown;
                                        agent.diagnostic_code = Some(error);
                                    }
                                    snapshot.last_refresh_ms = None;
                                    snapshot.collection_error = Some(error);
                                    if let Some(result) = history_result {
                                        snapshot.storage_error = result.err().map(storage_code);
                                    }
                                }
                            }
                        }
                        state
                            .snapshot
                            .lock()
                            .expect("state lock poisoned")
                            .refreshing = false;
                        let completed = state.snapshot();
                        state.observe_monitor_sources(&completed);
                        state.record_diagnostic(
                            crate::diagnostics::EventCode::RefreshCompleted,
                            started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                            completed.agents.len() as u64,
                        );
                        state.process_alerts(&previous, &completed);
                        *state.in_flight.lock().expect("refresh lock poisoned") = None;
                        state.publish();
                        sender.send_modify(|result| *result = Some(completed));
                    });
                    receiver
                }
            }
        };
        loop {
            if let Some(result) = receiver.borrow().clone() {
                return result;
            }
            if receiver.changed().await.is_err() {
                return self.snapshot();
            }
        }
    }
    pub async fn stop(&self) {
        self.shutdown.send_replace(true);
        self.stop_metrics();
        let active = self
            .in_flight
            .lock()
            .expect("refresh lock poisoned")
            .clone();
        if let Some(mut receiver) = active {
            while receiver.borrow().is_none() {
                if receiver.changed().await.is_err() {
                    break;
                }
            }
        }
        self.wait_for_wake().await;
        self.stop_diagnostics();
        *self.publisher.lock().expect("publisher lock poisoned") = None;
    }
}

fn storage_code(error: StorageError) -> DiagnosticCode {
    match error {
        StorageError::Incompatible => DiagnosticCode::StorageIncompatible,
        StorageError::Corrupt => DiagnosticCode::StorageCorrupt,
        _ => DiagnosticCode::StorageUnavailable,
    }
}

pub fn now_ms() -> i64 {
    crate::environment::timestamp_ms(std::time::SystemTime::now())
        .expect("System clock is outside the supported UTC range")
}

pub struct DiscoveryCollector {
    pub search_dirs: Vec<std::path::PathBuf>,
}
impl Collector for DiscoveryCollector {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async {
            use crate::adapters::{claude::ClaudeAdapter, codex::CodexAdapter, AgentAdapter};
            let cwd =
                tempfile::tempdir().map_err(|_| DiagnosticCode::TemporaryDirectoryUnavailable)?;
            let context = crate::discovery::DiscoveryContext {
                search_dirs: self.search_dirs.clone(),
                isolated_cwd: cwd.path().into(),
            };
            let observed = now_ms();
            let (claude, codex) = tokio::join!(
                ClaudeAdapter.detect(&context, observed),
                CodexAdapter.detect(&context, observed)
            );
            Ok(vec![claude, codex])
        })
    }
}
mod capture_watch;
mod preferences;
mod pricing;
pub use pricing::PricingView;
