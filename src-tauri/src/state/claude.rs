use super::AppState;
use crate::{
    claude_bridge::control::{BridgeControl, BridgeIssue, BridgeReview, BridgeStatus},
    model::{AgentId, AgentState},
};
use std::sync::Arc;
impl AppState {
    pub fn set_claude_bridge(&self, control: Arc<BridgeControl>) {
        *self
            .claude_bridge
            .lock()
            .expect("Claude bridge lock poisoned") = Some(control);
    }
    fn claude_control(&self) -> Result<Arc<BridgeControl>, BridgeIssue> {
        self.claude_bridge
            .lock()
            .expect("Claude bridge lock poisoned")
            .clone()
            .ok_or(BridgeIssue::SettingsUnavailable)
    }
    pub fn claude_bridge_status(&self) -> Result<BridgeStatus, BridgeIssue> {
        Ok(self.claude_control()?.status(super::now_ms()))
    }
    pub fn prepare_claude_bridge(&self) -> Result<BridgeReview, BridgeIssue> {
        self.claude_control()?.prepare(super::now_ms())
    }
    pub fn apply_claude_bridge(
        &self,
        id: &str,
        confirmed: bool,
    ) -> Result<BridgeStatus, BridgeIssue> {
        self.claude_control()?.apply(id, confirmed, super::now_ms())
    }
    pub fn restore_claude_bridge(&self, confirmed: bool) -> Result<BridgeStatus, BridgeIssue> {
        self.claude_control()?.restore(confirmed, super::now_ms())
    }
    pub(super) fn attach_claude_capture(&self, agents: &mut [AgentState]) {
        let Ok(control) = self.claude_control() else {
            return;
        };
        let now = super::now_ms();
        if control.status(now).configured != Some(true) {
            return;
        }
        if let Some(agent) = agents.iter_mut().find(|v| v.agent_id == AgentId::Claude) {
            if let Err(error) =
                crate::claude_bridge::runtime::attach_capture(agent, control.capture_dir(), now)
            {
                eprintln!("Jentrics Claude capture: {error:?}");
            }
        }
    }
    pub fn refresh_claude_capture(&self) {
        let mut storage = self.storage.lock().expect("storage lock poisoned");
        let mut snapshot = self.snapshot.lock().expect("state lock poisoned");
        let before = snapshot.clone();
        let mut agents = before.agents.clone();
        self.attach_claude_capture(&mut agents);
        if agents == before.agents {
            return;
        }
        let storage_result = storage.as_mut().map(|storage| storage.save_agents(&agents));
        snapshot.agents = agents;
        if let Some(result) = storage_result {
            snapshot.storage_error = result.err().map(super::storage_code);
        }
        drop(snapshot);
        drop(storage);
        self.process_alerts(&before, &self.snapshot());
        self.publish();
    }
}
