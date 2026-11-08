use super::{settings, Storage, StorageError};
use crate::model::{AgentId, AgentState, DiagnosticCode};
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Serialize};

pub(super) fn enum_text(value: &impl Serialize) -> Result<String, StorageError> {
    serde_json::to_value(value)
        .map_err(|_| StorageError::InvalidState)?
        .as_str()
        .map(str::to_owned)
        .ok_or(StorageError::InvalidState)
}

pub(super) fn parse_enum<T: DeserializeOwned>(value: String) -> Result<T, StorageError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| StorageError::Corrupt)
}

fn insert_check(
    connection: &Connection,
    agent_id: AgentId,
    observed_ms: i64,
    status: &str,
    code: Option<DiagnosticCode>,
) -> Result<(), StorageError> {
    let diagnostic = code.as_ref().map(enum_text).transpose()?;
    connection.execute("INSERT INTO collection_checks(agent_id,observed_ms,status,diagnostic_code) VALUES(?1,?2,?3,?4) ON CONFLICT(agent_id,observed_ms) DO UPDATE SET status=excluded.status,diagnostic_code=excluded.diagnostic_code",params![enum_text(&agent_id)?,observed_ms,status,diagnostic])
        .map_err(|_| StorageError::Unavailable)?;
    Ok(())
}

pub(super) fn append(connection: &Connection, agent: &AgentState) -> Result<(), StorageError> {
    let diagnostic = agent
        .quota
        .as_ref()
        .map_or(agent.diagnostic_code, |quota| quota.diagnostic_code);
    let check_observed = agent
        .quota
        .as_ref()
        .filter(|quota| quota.source == crate::model::QuotaSource::ClaudeStatusLine)
        .map_or(agent.observed_ms, |quota| quota.observed_ms);
    let status = if diagnostic.is_some() {
        "error"
    } else if agent
        .quota
        .as_ref()
        .is_some_and(|quota| !quota.windows.is_empty())
    {
        "reported"
    } else {
        "unavailable"
    };
    insert_check(
        connection,
        agent.agent_id,
        check_observed,
        status,
        diagnostic,
    )?;
    if let Some(quota) = &agent.quota {
        for window in &quota.windows {
            connection.execute("INSERT INTO quota_samples(agent_id,bucket_id,observed_ms,used_percent,window_minutes,resets_at_ms,source) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(agent_id,bucket_id,observed_ms) DO UPDATE SET used_percent=excluded.used_percent,window_minutes=excluded.window_minutes,resets_at_ms=excluded.resets_at_ms,source=excluded.source",params![enum_text(&agent.agent_id)?,window.bucket_id,window.observed_ms,window.used_percent,window.window_minutes,window.resets_at_ms,enum_text(&quota.source)?])
                .map_err(|_| StorageError::Unavailable)?;
        }
    }
    Ok(())
}

impl Storage {
    pub fn record_history_failure(
        &mut self,
        agent_ids: &[AgentId],
        observed_ms: i64,
        code: DiagnosticCode,
    ) -> Result<(), StorageError> {
        settings::validate_timestamp(observed_ms)?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        for agent_id in agent_ids {
            insert_check(&transaction, *agent_id, observed_ms, "error", Some(code))?;
        }
        let deleted = settings::prune(&transaction, observed_ms)?;
        transaction
            .commit()
            .map_err(|_| StorageError::Unavailable)?;
        if deleted > 0 {
            self.finish_maintenance(super::MaintenanceOperation::QuotaRetention);
        }
        Ok(())
    }
}
