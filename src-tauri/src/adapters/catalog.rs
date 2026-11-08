use crate::model::{AgentId, AgentState, Availability, DashboardState};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterId {
    Claude,
    Codex,
    Gemini,
    OpenCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InterfaceSource {
    ClaudeAuthStatus,
    ClaudeStatusLine,
    ClaudeOtlpHttpJson,
    CodexAppServer,
    GeminiOtlpHttpJson,
    OpenCodeStatsV2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Qualification {
    Unchecked,
    NotInstalled,
    UnsupportedVersion,
    ReadOnlyProfile,
    FixtureQualified,
    SourceContractOnly,
    StartupIsolationUnqualified,
    InterfaceUnavailable,
}

/// Fields admitted by this source contract, not a promise that an observation
/// contains them. Qualification and execution gates remain separate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceCapabilities {
    pub readiness: bool,
    pub quota_windows: bool,
    pub reset_timestamps: bool,
    pub tokens: bool,
    pub account_usage: bool,
    pub model_ids: bool,
    pub request_duration: bool,
    pub mean_request_latency: bool,
    pub active_time: bool,
    pub time_to_first_token: bool,
    pub output_rate: bool,
    pub provider_error_counts: bool,
    pub incremental_usage_cursor: bool,
    pub start_session: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInterface {
    pub adapter_id: AdapterId,
    pub adapter_version: u8,
    pub source: InterfaceSource,
    pub cli_version: Option<String>,
    pub qualification: Qualification,
    pub requires_opt_in: bool,
    /// Permission to invoke the native CLI through this interface. False for
    /// passive captures/exports, even when their parser contract is qualified.
    pub native_collection_allowed: bool,
    pub capabilities: InterfaceCapabilities,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterCatalog {
    pub schema_version: u8,
    pub interfaces: Vec<AdapterInterface>,
}

fn native_interface(
    snapshot: &DashboardState,
    id: AgentId,
    source: InterfaceSource,
) -> AdapterInterface {
    let state = snapshot.agents.iter().find(|state| state.agent_id == id);
    let version = state.and_then(|state| state.version.as_deref());
    let known = version.is_some_and(|version| crate::discovery::version_supported(id, version));
    let qualification = match state {
        None => Qualification::Unchecked,
        Some(AgentState {
            availability: Availability::NotFound,
            ..
        }) => Qualification::NotInstalled,
        Some(_) if !known => Qualification::UnsupportedVersion,
        Some(AgentState {
            availability: Availability::Unsupported,
            ..
        }) => Qualification::InterfaceUnavailable,
        Some(_) if id == AgentId::Claude => Qualification::FixtureQualified,
        Some(_) => Qualification::ReadOnlyProfile,
    };
    let profile_allowed =
        known && state.is_some_and(|state| state.availability == Availability::Detected);
    let usage = id == AgentId::Codex && version.is_some_and(crate::codex_usage::usage_supported);
    AdapterInterface {
        adapter_id: if id == AgentId::Claude {
            AdapterId::Claude
        } else {
            AdapterId::Codex
        },
        adapter_version: 1,
        source,
        cli_version: version.map(str::to_owned),
        qualification,
        requires_opt_in: false,
        native_collection_allowed: profile_allowed,
        capabilities: InterfaceCapabilities {
            readiness: known,
            quota_windows: known && id == AgentId::Codex,
            reset_timestamps: known && id == AgentId::Codex,
            tokens: usage,
            account_usage: usage,
            ..InterfaceCapabilities::default()
        },
    }
}

pub fn catalog(snapshot: &DashboardState) -> AdapterCatalog {
    AdapterCatalog {
        schema_version: 1,
        interfaces: vec![
            native_interface(snapshot, AgentId::Claude, InterfaceSource::ClaudeAuthStatus),
            AdapterInterface {
                adapter_id: AdapterId::Claude,
                adapter_version: 1,
                source: InterfaceSource::ClaudeStatusLine,
                cli_version: None,
                qualification: Qualification::SourceContractOnly,
                requires_opt_in: true,
                native_collection_allowed: false,
                capabilities: InterfaceCapabilities {
                    quota_windows: true,
                    reset_timestamps: true,
                    ..InterfaceCapabilities::default()
                },
            },
            AdapterInterface {
                adapter_id: AdapterId::Claude,
                adapter_version: 1,
                source: InterfaceSource::ClaudeOtlpHttpJson,
                cli_version: None,
                qualification: Qualification::SourceContractOnly,
                requires_opt_in: true,
                native_collection_allowed: false,
                capabilities: InterfaceCapabilities {
                    tokens: true,
                    model_ids: true,
                    active_time: true,
                    ..InterfaceCapabilities::default()
                },
            },
            native_interface(snapshot, AgentId::Codex, InterfaceSource::CodexAppServer),
            AdapterInterface {
                adapter_id: AdapterId::Gemini,
                adapter_version: 1,
                source: InterfaceSource::GeminiOtlpHttpJson,
                cli_version: None,
                qualification: Qualification::SourceContractOnly,
                requires_opt_in: true,
                native_collection_allowed: false,
                capabilities: InterfaceCapabilities {
                    tokens: true,
                    model_ids: true,
                    mean_request_latency: true,
                    provider_error_counts: true,
                    ..InterfaceCapabilities::default()
                },
            },
            AdapterInterface {
                adapter_id: AdapterId::OpenCode,
                adapter_version: 1,
                source: InterfaceSource::OpenCodeStatsV2,
                cli_version: None,
                qualification: Qualification::StartupIsolationUnqualified,
                requires_opt_in: false,
                native_collection_allowed: false,
                capabilities: InterfaceCapabilities {
                    tokens: true,
                    ..InterfaceCapabilities::default()
                },
            },
        ],
    }
}
