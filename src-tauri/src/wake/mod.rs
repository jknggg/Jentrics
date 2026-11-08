use crate::model::{AgentId, DashboardState};
use serde::{Deserialize, Serialize};
use std::{future::Future, path::PathBuf, pin::Pin};

pub mod eligibility;
pub mod provider;
pub const WAKE_PROMPT: &str = "Reply only with hello.";
pub const MODEL_LABEL: &str = "Provider default (subscription)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WakeReason {
    NotInstalled,
    Unsupported,
    RetryPolicyUnsupported,
    SignInRequired,
    SubscriptionRequired,
    ActiveWindow,
    WeeklyOnly,
    Busy,
    Cooldown,
    Stopped,
    ConfirmationRequired,
    PrepareRequired,
    ExpiredConfirmation,
    ProviderUnavailable,
    TimedOut,
    OutputLimit,
    RequestFailed,
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeOffer {
    pub agent_id: AgentId,
    pub enabled: bool,
    pub reason: Option<WakeReason>,
    pub model_label: String,
    pub prompt: String,
    pub quota_unverified: bool,
    pub confirmation_id: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WakeStatus {
    Completed,
    Failed,
    TimedOut,
    Cancelled,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuotaEffect {
    NewWindowObserved,
    Unchanged,
    Unverified,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeOutcome {
    pub agent_id: AgentId,
    pub status: WakeStatus,
    pub reason: Option<WakeReason>,
    pub quota_effect: QuotaEffect,
    pub dashboard: DashboardState,
}
#[derive(Debug, Clone)]
pub struct PreparedWake {
    pub agent_id: AgentId,
    pub executable: PathBuf,
    pub version: String,
}
pub trait WakeRunner: Send + Sync {
    fn prepare(
        &self,
        agent: AgentId,
    ) -> Pin<Box<dyn Future<Output = Result<PreparedWake, WakeReason>> + Send + '_>>;
    fn run(
        &self,
        prepared: PreparedWake,
    ) -> Pin<Box<dyn Future<Output = Result<(), WakeReason>> + Send + '_>>;
}
