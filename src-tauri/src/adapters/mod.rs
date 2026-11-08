mod catalog;
pub mod claude;
use crate::{
    discovery::DiscoveryContext,
    model::{AgentId, AgentState, Capabilities},
};
pub use catalog::{
    catalog, AdapterCatalog, AdapterId, AdapterInterface, InterfaceCapabilities, InterfaceSource,
    Qualification,
};

pub trait AgentAdapter: Sync {
    fn agent_id(&self) -> AgentId;
    fn capabilities(&self, version: &str) -> Capabilities {
        Capabilities {
            discovery: true,
            auth_readiness: crate::discovery::version_supported(self.agent_id(), version),
            ..Default::default()
        }
    }
    fn detect(
        &self,
        context: &DiscoveryContext,
        observed_ms: i64,
    ) -> impl std::future::Future<Output = AgentState> + Send {
        self.read_snapshot(context, observed_ms)
    }
    fn read_snapshot(
        &self,
        context: &DiscoveryContext,
        observed_ms: i64,
    ) -> impl std::future::Future<Output = AgentState> + Send {
        crate::discovery::discover_agent(self.agent_id(), context, observed_ms)
    }
    /// These native interfaces supply account/readiness snapshots. They expose
    /// no provider usage cursor; local telemetry history is a separate source.
    fn read_usage_since(
        &self,
        _cursor: Option<&str>,
    ) -> impl std::future::Future<Output = Result<UsagePage, IncrementalUsageError>> + Send {
        std::future::ready(Err(IncrementalUsageError::SourceHasNoCursor))
    }
}
pub mod codex;

#[derive(Debug)]
pub struct UsagePage {
    pub events: Vec<crate::telemetry::TelemetryDelta>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalUsageError {
    SourceHasNoCursor,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AdapterError {
    Malformed,
    OutOfRange,
}
