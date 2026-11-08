use super::AdapterError;
pub struct ClaudeAdapter;
impl super::AgentAdapter for ClaudeAdapter {
    fn agent_id(&self) -> crate::model::AgentId {
        crate::model::AgentId::Claude
    }
}
use crate::{model::AuthState, process::ProbeOutput};
pub fn parse_auth(output: &ProbeOutput) -> Result<AuthState, AdapterError> {
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| AdapterError::Malformed)?;
    match (
        output.exit_code,
        value.get("loggedIn").and_then(serde_json::Value::as_bool),
    ) {
        (Some(0), Some(true)) => Ok(AuthState::Ready),
        (Some(1), Some(false)) => Ok(AuthState::NotAuthenticated),
        _ => Err(AdapterError::Malformed),
    }
}
