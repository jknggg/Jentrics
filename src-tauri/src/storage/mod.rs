use crate::model::AgentState;
use rusqlite::{params, Connection};
use std::path::Path;
mod configuration;
mod history_query;
mod history_write;
mod maintenance;
mod migrations;
mod preferences;
mod quota;
mod settings;
pub use maintenance::{
    MaintenanceError, MaintenanceOperation, MaintenanceOutcome, MaintenanceStatus,
};
pub use preferences::PreferenceKey;
pub(crate) use settings::validate_days as validate_retention;
// Account daily buckets share the bounded 64-KiB RPC response budget. Allow
// its normalized representation plus the readiness/quota wrapper without truncation.
const MAX_STATE_BYTES: usize = 128 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum StorageError {
    Unavailable,
    Incompatible,
    Corrupt,
    InvalidState,
}
pub struct Storage {
    connection: Connection,
    maintenance: MaintenanceStatus,
}
impl Storage {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let mut connection = Connection::open(path).map_err(|_| StorageError::Unavailable)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| StorageError::Unavailable)?;
        migrations::apply(&mut connection)?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|_| StorageError::Unavailable)?;
        Ok(Self {
            connection,
            maintenance: MaintenanceStatus::default(),
        })
    }
    pub fn load_agents(&self) -> Result<Vec<AgentState>, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT state_json FROM agent_state ORDER BY agent_id")
            .map_err(|_| StorageError::Corrupt)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| StorageError::Corrupt)?;
        let mut result = Vec::new();
        for row in rows {
            let encoded = row.map_err(|_| StorageError::Corrupt)?;
            if encoded.len() > MAX_STATE_BYTES {
                return Err(StorageError::Corrupt);
            }
            let state: AgentState =
                serde_json::from_str(&encoded).map_err(|_| StorageError::Corrupt)?;
            validate(&state).map_err(|_| StorageError::Corrupt)?;
            result.push(state);
            if result.len() > 2 {
                return Err(StorageError::Corrupt);
            }
        }
        Ok(result)
    }
    pub fn save_agents(&mut self, agents: &[AgentState]) -> Result<(), StorageError> {
        for agent in agents {
            validate(agent)?;
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        for agent in agents {
            let id = match agent.agent_id {
                crate::model::AgentId::Claude => "claude",
                crate::model::AgentId::Codex => "codex",
            };
            let encoded = serde_json::to_string(agent).map_err(|_| StorageError::InvalidState)?;
            if encoded.len() > MAX_STATE_BYTES {
                return Err(StorageError::InvalidState);
            }
            transaction.execute("INSERT INTO agent_state (agent_id,state_json) VALUES (?1,?2) ON CONFLICT(agent_id) DO UPDATE SET state_json=excluded.state_json",params![id,encoded]).map_err(|_|StorageError::Unavailable)?;
            history_write::append(&transaction, agent)?;
        }
        let deleted = if let Some(now_ms) = agents.iter().map(|agent| agent.observed_ms).max() {
            settings::prune(&transaction, now_ms)?
        } else {
            0
        };
        transaction
            .commit()
            .map_err(|_| StorageError::Unavailable)?;
        if deleted > 0 {
            self.finish_maintenance(MaintenanceOperation::QuotaRetention);
        }
        Ok(())
    }
}

fn validate(agent: &AgentState) -> Result<(), StorageError> {
    if settings::validate_timestamp(agent.observed_ms).is_err()
        || agent
            .version
            .as_ref()
            .is_some_and(|v| !crate::discovery::valid_version(v))
        || agent.capabilities.quotas != agent.quota.is_some()
        || agent.capabilities.usage != agent.account_usage.is_some()
        || agent.capabilities.session_start
    {
        return Err(StorageError::InvalidState);
    }
    quota::validate(agent)?;
    if let Some(usage) = &agent.account_usage {
        if agent.agent_id != crate::model::AgentId::Codex
            || usage.data.is_some() == usage.error.is_some()
        {
            return Err(StorageError::InvalidState);
        }
        if let Some(data) = &usage.data {
            let checked=crate::codex_usage::normalize_response(&serde_json::json!({"result":{"summary":data.summary,"dailyUsageBuckets":data.daily_usage_buckets}}),data.observed_ms).map_err(|_|StorageError::InvalidState)?;
            if &checked != data {
                return Err(StorageError::InvalidState);
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod disk_full_tests;
#[cfg(test)]
mod maintenance_tests;
mod metrics;
