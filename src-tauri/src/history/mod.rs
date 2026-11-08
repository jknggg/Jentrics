use crate::model::{AgentId, DiagnosticCode};
use serde::{Deserialize, Serialize};

pub type HistoryRange = u16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySample {
    pub agent_id: AgentId,
    pub bucket_id: String,
    pub observed_ms: i64,
    pub used_percent: Option<f64>,
    pub window_minutes: Option<i64>,
    pub resets_at_ms: Option<i64>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCheck {
    pub agent_id: AgentId,
    pub observed_ms: i64,
    pub status: String,
    pub diagnostic_code: Option<DiagnosticCode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    pub range_hours: HistoryRange,
    pub from_ms: i64,
    pub until_ms: i64,
    pub retention_days: u16,
    pub samples: Vec<HistorySample>,
    pub checks: Vec<HistoryCheck>,
    pub truncated: bool,
    pub error: Option<DiagnosticCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySettings {
    pub retention_days: u16,
}
