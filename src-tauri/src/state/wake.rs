use super::AppState;
use crate::{
    model::AgentId,
    wake::{
        eligibility, PreparedWake, QuotaEffect, WakeOffer, WakeOutcome, WakeReason, WakeRunner,
        WakeStatus, MODEL_LABEL, WAKE_PROMPT,
    },
};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::watch;

struct Preparation {
    id: String,
    created: Instant,
    prepared: PreparedWake,
}
#[derive(Default)]
struct Slot {
    preparing: bool,
    prepared: Option<Preparation>,
    running: Option<watch::Receiver<Option<WakeOutcome>>>,
    cooldown: Option<Instant>,
}
pub(super) struct WakeControl {
    runner: Mutex<Option<Arc<dyn WakeRunner>>>,
    slots: Mutex<[Slot; 2]>,
    next_id: AtomicU64,
}
impl Default for WakeControl {
    fn default() -> Self {
        Self {
            runner: Mutex::new(None),
            slots: Mutex::new(std::array::from_fn(|_| Slot::default())),
            next_id: AtomicU64::new(1),
        }
    }
}
fn index(id: AgentId) -> usize {
    match id {
        AgentId::Claude => 0,
        AgentId::Codex => 1,
    }
}
fn blocked(slot: &Slot) -> Option<WakeReason> {
    if slot.preparing || slot.running.is_some() {
        Some(WakeReason::Busy)
    } else if slot.cooldown.is_some_and(|until| until > Instant::now()) {
        Some(WakeReason::Cooldown)
    } else {
        None
    }
}
fn offer(
    id: AgentId,
    reason: Option<WakeReason>,
    unknown: bool,
    confirmation_id: Option<String>,
) -> WakeOffer {
    WakeOffer {
        agent_id: id,
        enabled: reason.is_none(),
        reason,
        model_label: MODEL_LABEL.into(),
        prompt: WAKE_PROMPT.into(),
        quota_unverified: unknown,
        confirmation_id,
    }
}
// A closed window or cancelled prepare invocation must not leave its provider busy.
struct PrepareGuard<'a> {
    control: &'a WakeControl,
    index: usize,
}
impl Drop for PrepareGuard<'_> {
    fn drop(&mut self) {
        self.control.slots.lock().expect("wake lock poisoned")[self.index].preparing = false;
    }
}
impl AppState {
    pub fn set_wake_runner(&self, runner: Arc<dyn WakeRunner>) {
        *self.wake.runner.lock().expect("runner lock poisoned") = Some(runner);
    }
    pub async fn prepare_wake(self: &Arc<Self>, id: AgentId) -> WakeOffer {
        let position = index(id);
        {
            let mut slots = self.wake.slots.lock().expect("wake lock poisoned");
            if *self.shutdown.borrow() {
                return offer(id, Some(WakeReason::Stopped), true, None);
            }
            if let Some(reason) = blocked(&slots[position]) {
                return offer(id, Some(reason), true, None);
            }
            slots[position].preparing = true;
            slots[position].prepared = None;
        }
        let _guard = PrepareGuard {
            control: &self.wake,
            index: position,
        };
        let runner = self
            .wake
            .runner
            .lock()
            .expect("runner lock poisoned")
            .clone();
        let Some(runner) = runner else {
            return offer(id, Some(WakeReason::ProviderUnavailable), true, None);
        };
        let mut shutdown = self.shutdown.subscribe();
        if *shutdown.borrow() {
            return offer(id, Some(WakeReason::Stopped), true, None);
        }
        let result = async {
            let fresh = self.refresh().await;
            let unknown = eligibility::check(&fresh, id, super::now_ms())?;
            let prepared = runner.prepare(id).await?;
            let confirmation_id = self
                .wake
                .next_id
                .fetch_add(1, Ordering::Relaxed)
                .to_string();
            self.wake.slots.lock().expect("wake lock poisoned")[position].prepared =
                Some(Preparation {
                    id: confirmation_id.clone(),
                    created: Instant::now(),
                    prepared,
                });
            Ok::<_, WakeReason>(offer(id, None, unknown, Some(confirmation_id)))
        };
        tokio::select! {
            biased;
            _=shutdown.changed()=>offer(id,Some(WakeReason::Stopped),true,None),
            result=result=>result.unwrap_or_else(|reason|offer(id,Some(reason),true,None)),
        }
    }
    fn wake_result(
        &self,
        id: AgentId,
        status: WakeStatus,
        reason: Option<WakeReason>,
    ) -> WakeOutcome {
        WakeOutcome {
            agent_id: id,
            status,
            reason,
            quota_effect: QuotaEffect::Unverified,
            dashboard: self.snapshot(),
        }
    }
    pub async fn confirm_wake(
        self: &Arc<Self>,
        id: AgentId,
        confirmation_id: String,
        confirmed: bool,
    ) -> WakeOutcome {
        let position = index(id);
        let (preparation, sender, mut receiver) = {
            let mut slots = self.wake.slots.lock().expect("wake lock poisoned");
            let slot = &mut slots[position];
            if !confirmed {
                if slot
                    .prepared
                    .as_ref()
                    .is_some_and(|p| p.id == confirmation_id)
                {
                    slot.prepared = None;
                }
                return self.wake_result(id, WakeStatus::Cancelled, Some(WakeReason::Cancelled));
            }
            if *self.shutdown.borrow() {
                return self.wake_result(id, WakeStatus::Unavailable, Some(WakeReason::Stopped));
            }
            if let Some(reason) = blocked(slot) {
                return self.wake_result(id, WakeStatus::Unavailable, Some(reason));
            }
            let Some(preparation) = slot.prepared.take().filter(|p| p.id == confirmation_id) else {
                return self.wake_result(
                    id,
                    WakeStatus::Unavailable,
                    Some(WakeReason::PrepareRequired),
                );
            };
            if preparation.created.elapsed() > Duration::from_secs(120) {
                return self.wake_result(
                    id,
                    WakeStatus::Unavailable,
                    Some(WakeReason::ExpiredConfirmation),
                );
            }
            let (sender, receiver) = watch::channel(None);
            slot.running = Some(receiver.clone());
            (preparation, sender, receiver)
        };
        let state = self.clone();
        tokio::spawn(async move {
            let result = state.execute_wake(id, preparation.prepared).await;
            state.wake.slots.lock().expect("wake lock poisoned")[position].running = None;
            sender.send_replace(Some(result));
        });
        loop {
            if let Some(result) = receiver.borrow().clone() {
                return result;
            }
            if receiver.changed().await.is_err() {
                return self.wake_result(id, WakeStatus::Failed, Some(WakeReason::RequestFailed));
            }
        }
    }
    async fn execute_wake(self: &Arc<Self>, id: AgentId, prepared: PreparedWake) -> WakeOutcome {
        let mut shutdown = self.shutdown.subscribe();
        if *shutdown.borrow() {
            return self.wake_result(id, WakeStatus::Cancelled, Some(WakeReason::Stopped));
        }
        let before = self.refresh().await;
        if *shutdown.borrow() {
            return self.wake_result(id, WakeStatus::Cancelled, Some(WakeReason::Stopped));
        }
        if let Err(reason) = eligibility::check(&before, id, super::now_ms()) {
            return self.wake_result(id, WakeStatus::Unavailable, Some(reason));
        }
        let runner = self
            .wake
            .runner
            .lock()
            .expect("runner lock poisoned")
            .clone();
        let Some(runner) = runner else {
            return self.wake_result(
                id,
                WakeStatus::Unavailable,
                Some(WakeReason::ProviderUnavailable),
            );
        };
        self.wake.slots.lock().expect("wake lock poisoned")[index(id)].cooldown =
            Some(Instant::now() + Duration::from_secs(60));
        let result = tokio::select! {
            biased;
            _=shutdown.changed()=>return self.wake_result(id,WakeStatus::Cancelled,Some(WakeReason::Cancelled)),
            result=runner.run(prepared)=>result,
        };
        // Always re-read quota once after an attempted request, including uncertain timeout outcomes.
        let after = self.refresh().await;
        match result {
            Ok(()) => WakeOutcome {
                agent_id: id,
                status: WakeStatus::Completed,
                reason: None,
                quota_effect: eligibility::quota_effect(&before, &after, id, super::now_ms()),
                dashboard: after,
            },
            Err(reason) => {
                let status = match reason {
                    WakeReason::TimedOut => WakeStatus::TimedOut,
                    WakeReason::Cancelled | WakeReason::Stopped => WakeStatus::Cancelled,
                    WakeReason::Unsupported
                    | WakeReason::RetryPolicyUnsupported
                    | WakeReason::SubscriptionRequired
                    | WakeReason::SignInRequired
                    | WakeReason::NotInstalled => WakeStatus::Unavailable,
                    _ => WakeStatus::Failed,
                };
                self.wake_result(id, status, Some(reason))
            }
        }
    }
    pub(super) async fn wait_for_wake(&self) {
        let receivers: Vec<_> = self
            .wake
            .slots
            .lock()
            .expect("wake lock poisoned")
            .iter()
            .filter_map(|slot| slot.running.clone())
            .collect();
        for mut receiver in receivers {
            while receiver.borrow().is_none() {
                if receiver.changed().await.is_err() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{AgentState, DiagnosticCode},
        state::Collector,
        storage::StorageError,
    };
    struct NoCollection;
    impl Collector for NoCollection {
        fn collect(
            &self,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Vec<AgentState>, DiagnosticCode>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async { panic!("expired confirmation must not start provider collection") })
        }
    }
    #[tokio::test]
    async fn expired_confirmation_is_consumed_without_a_provider_request() {
        let state = AppState::new(Err(StorageError::Unavailable), Arc::new(NoCollection));
        state.wake.slots.lock().unwrap()[index(AgentId::Codex)].prepared = Some(Preparation {
            id: "expired".into(),
            created: Instant::now() - Duration::from_secs(121),
            prepared: PreparedWake {
                agent_id: AgentId::Codex,
                executable: std::env::current_exe().unwrap(),
                version: "0.165.0-alpha.1.2".into(),
            },
        });
        let result = state
            .confirm_wake(AgentId::Codex, "expired".into(), true)
            .await;
        assert_eq!(result.reason, Some(WakeReason::ExpiredConfirmation));
        assert!(state.wake.slots.lock().unwrap()[index(AgentId::Codex)]
            .prepared
            .is_none());
    }
}
