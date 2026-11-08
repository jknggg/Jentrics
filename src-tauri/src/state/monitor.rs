use super::AppState;
use crate::model::{AgentId, Availability, DashboardState};
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
mod settings;
pub(crate) use settings::MonitorControl;
pub use settings::{MonitorError, MonitorRuntimeReason, MonitorSettings, MonitorSettingsView};

pub const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

impl AppState {
    /// Production scheduler: unreadable settings suspend automatic reads rather
    /// than silently substituting the first-install default. Manual refresh is
    /// independent. One completion-delayed read runs on startup/resume; missed
    /// periods are never replayed.
    pub async fn run_monitor(self: Arc<Self>) {
        let mut shutdown = self.shutdown.subscribe();
        let mut changed = self.monitor.changes.subscribe();
        let mut failures = 0u8;
        loop {
            changed.borrow_and_update();
            if *shutdown.borrow() {
                return;
            }
            let view = self.monitor_settings();
            let Some(settings) = view.settings.filter(|settings| {
                !settings.paused
                    && view.runtime_reason != Some(MonitorRuntimeReason::NoQualifiedProviders)
            }) else {
                {
                    let mut runtime = self
                        .monitor
                        .runtime
                        .lock()
                        .expect("monitor runtime lock poisoned");
                    runtime.next_check_ms = None;
                    runtime.effective_minutes = None;
                }
                tokio::select! {biased; _=shutdown.changed()=>return, _=changed.changed()=>{failures=0;continue;}}
            };
            let recovered = {
                let mut runtime = self
                    .monitor
                    .runtime
                    .lock()
                    .expect("monitor runtime lock poisoned");
                std::mem::take(&mut runtime.recovered_observation)
            };
            let result = if recovered {
                self.snapshot()
            } else {
                self.refresh().await
            };
            if *shutdown.borrow() {
                return;
            }
            if self.monitor_settings().runtime_reason.is_some() {
                continue;
            }
            failures = if collection_failed(&result) {
                failures.saturating_add(1)
            } else {
                0
            };
            let minutes = backoff_minutes(settings.interval_minutes, failures);
            let period = Duration::from_secs(u64::from(minutes) * 60);
            {
                let mut runtime = self
                    .monitor
                    .runtime
                    .lock()
                    .expect("monitor runtime lock poisoned");
                runtime.next_check_ms = Some(super::now_ms() + i64::from(minutes) * 60000);
                runtime.effective_minutes = Some(minutes);
            }
            self.publish();
            let deadline = tokio::time::Instant::now() + period;
            let mut wall = SystemTime::now();
            let mut monotonic = Instant::now();
            loop {
                // A bounded clock probe detects wall/monotonic disagreement
                // even on platforms whose monotonic clock pauses during sleep.
                // Resume detection is practical (within 30s after scheduling),
                // not an OS suspend event guarantee.
                let probe = deadline.min(tokio::time::Instant::now() + Duration::from_secs(30));
                tokio::select! {
                    biased;
                    _=shutdown.changed()=>return,
                    _=changed.changed()=>{failures=0;break;},
                    _=tokio::time::sleep_until(probe)=>{
                        let next_wall=SystemTime::now();let next_monotonic=Instant::now();
                        let shifted=timing_discontinuity(next_wall.duration_since(wall).ok(),next_monotonic.duration_since(monotonic),period);
                        wall=next_wall;monotonic=next_monotonic;
                        if shifted {self.monitor.runtime.lock().expect("monitor runtime lock poisoned").last_clock_change_ms=Some(super::now_ms());failures=0;break;}
                        if tokio::time::Instant::now()>=deadline{break;}
                    }
                }
            }
        }
    }
    pub(super) fn observe_monitor_sources(&self, snapshot: &DashboardState) {
        if snapshot.collection_error.is_some() || snapshot.last_refresh_ms.is_none() {
            return;
        }
        let absent = [AgentId::Claude, AgentId::Codex].iter().all(|id| {
            snapshot.agents.iter().any(|agent| {
                agent.agent_id == *id
                    && matches!(
                        agent.availability,
                        Availability::NotFound | Availability::Unsupported
                    )
            })
        });
        let qualified = snapshot.agents.iter().any(|agent| {
            matches!(
                agent.availability,
                Availability::Detected | Availability::ProbeFailed
            )
        });
        if !absent && !qualified {
            return;
        }
        let transitioned = {
            let mut runtime = self
                .monitor
                .runtime
                .lock()
                .expect("monitor runtime lock poisoned");
            let recovered = runtime.no_qualified_providers && !absent;
            let transitioned = runtime.no_qualified_providers != absent;
            runtime.no_qualified_providers = absent;
            if absent {
                runtime.next_check_ms = None;
                runtime.effective_minutes = None;
            }
            runtime.recovered_observation |= recovered;
            transitioned
        };
        if transitioned {
            self.monitor
                .changes
                .send_modify(|revision| *revision = revision.wrapping_add(1));
            self.record_diagnostic(
                crate::diagnostics::EventCode::MonitorChanged,
                0,
                u64::from(absent),
            );
        }
    }
    /// Explicit fixed-period runner for integration fixtures and controlled
    /// callers. Production uses `run_monitor` with saved settings and backoff.
    pub async fn run_periodic_refresh(self: Arc<Self>, period: Duration) {
        assert!(
            !period.is_zero(),
            "automatic refresh period must be positive"
        );
        let mut shutdown = self.shutdown.subscribe();
        loop {
            if *shutdown.borrow() {
                return;
            }
            self.refresh().await;
            tokio::select! {
                biased;
                _ = shutdown.changed() => return,
                _ = tokio::time::sleep(period) => {},
            }
        }
    }
}

fn backoff_minutes(base: u16, failures: u8) -> u16 {
    base.saturating_mul(2u16.saturating_pow(u32::from(failures)))
        .min(30)
}
fn collection_failed(state: &DashboardState) -> bool {
    state.collection_error.is_some() || state.agents.iter().any(|agent| agent.monitoring_error())
}
fn timing_discontinuity(
    wall_elapsed: Option<Duration>,
    monotonic_elapsed: Duration,
    period: Duration,
) -> bool {
    wall_elapsed.is_none_or(|wall| wall.abs_diff(monotonic_elapsed) > Duration::from_secs(2))
        || monotonic_elapsed > period.saturating_mul(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DiagnosticCode;
    #[test]
    fn backoff_is_bounded_and_respects_the_selected_base() {
        assert_eq!(
            (0..5)
                .map(|failures| backoff_minutes(5, failures))
                .collect::<Vec<_>>(),
            vec![5, 10, 20, 30, 30]
        );
        assert_eq!(backoff_minutes(10, 1), 20);
        assert_eq!(backoff_minutes(15, 1), 30);
        assert_eq!(backoff_minutes(30, 100), 30);
    }
    #[test]
    fn clock_shift_and_late_resume_trigger_one_new_cycle_without_replay() {
        let period = Duration::from_secs(300);
        assert!(!timing_discontinuity(
            Some(Duration::from_secs(30)),
            Duration::from_secs(30),
            period
        ));
        assert!(timing_discontinuity(None, Duration::from_secs(30), period));
        assert!(timing_discontinuity(
            Some(Duration::from_secs(630)),
            Duration::from_secs(30),
            period
        ));
        assert!(timing_discontinuity(
            Some(Duration::from_secs(630)),
            Duration::from_secs(630),
            period
        ));
    }
    #[test]
    fn only_collection_or_quota_failures_trigger_backoff() {
        let mut state = DashboardState {
            schema_version: 1,
            agents: vec![],
            refreshing: false,
            last_refresh_ms: None,
            storage_error: Some(DiagnosticCode::StorageUnavailable),
            collection_error: None,
        };
        assert!(!collection_failed(&state));
        state.collection_error = Some(DiagnosticCode::TemporaryDirectoryUnavailable);
        assert!(collection_failed(&state));
    }
}
