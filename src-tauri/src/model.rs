use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentId {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    NotFound,
    Detected,
    Unsupported,
    ProbeFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthState {
    Ready,
    NotAuthenticated,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticCode {
    ExecutableUnavailable,
    VersionUnavailable,
    UnsupportedVersion,
    AuthUnavailable,
    ProbeTimeout,
    ProbeOutputLimit,
    StorageUnavailable,
    StorageIncompatible,
    StorageCorrupt,
    TemporaryDirectoryUnavailable,
    QuotaUnavailable,
    InvalidHistoryRequest,
    HistoryConfirmationRequired,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub discovery: bool,
    pub auth_readiness: bool,
    pub quotas: bool,
    pub usage: bool,
    pub session_start: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentState {
    pub agent_id: AgentId,
    pub version: Option<String>,
    pub availability: Availability,
    pub auth_state: AuthState,
    pub capabilities: Capabilities,
    pub observed_ms: i64,
    pub diagnostic_code: Option<DiagnosticCode>,
    pub quota: Option<QuotaSnapshot>,
    #[serde(default)]
    pub account_usage: Option<crate::codex_usage::UsageSnapshot>,
}
impl AgentState {
    pub fn monitoring_error(&self) -> bool {
        (self.availability == Availability::ProbeFailed && self.diagnostic_code.is_some())
            || self.diagnostic_code == Some(DiagnosticCode::QuotaUnavailable)
            || self
                .quota
                .as_ref()
                .is_some_and(|quota| quota.diagnostic_code.is_some())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuotaSnapshot {
    pub source: QuotaSource,
    pub observed_ms: i64,
    pub windows: Vec<crate::adapters::codex::QuotaWindow>,
    pub diagnostic_code: Option<DiagnosticCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuotaSource {
    CodexAppServer,
    ClaudeStatusLine,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardState {
    pub schema_version: u8,
    pub agents: Vec<AgentState>,
    pub refreshing: bool,
    pub last_refresh_ms: Option<i64>,
    pub storage_error: Option<DiagnosticCode>,
    pub collection_error: Option<DiagnosticCode>,
}
