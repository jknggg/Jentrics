use super::StorageError;
use crate::model::{AgentId, AgentState, DiagnosticCode, QuotaSource};

pub fn validate(agent: &AgentState) -> Result<(), StorageError> {
    let Some(quota) = &agent.quota else {
        return Ok(());
    };
    if !matches!(
        (&agent.agent_id, &quota.source),
        (AgentId::Codex, QuotaSource::CodexAppServer)
            | (AgentId::Claude, QuotaSource::ClaudeStatusLine)
    ) || quota.observed_ms < 0
        || quota.observed_ms > 8_640_000_000_000_000
        || (quota.source == QuotaSource::CodexAppServer && quota.observed_ms != agent.observed_ms)
        || quota.windows.len() > 32
        || quota.diagnostic_code.is_some_and(|code| {
            !matches!(
                code,
                DiagnosticCode::QuotaUnavailable
                    | DiagnosticCode::ProbeTimeout
                    | DiagnosticCode::ProbeOutputLimit
            )
        })
        || (quota.diagnostic_code.is_some() && !quota.windows.is_empty())
    {
        return Err(StorageError::InvalidState);
    }
    let mut ids = std::collections::HashSet::new();
    for window in &quota.windows {
        if window.bucket_id.is_empty()
            || window.bucket_id.len() > 138
            || !ids.insert(&window.bucket_id)
            || window.observed_ms != quota.observed_ms
            || window
                .used_percent
                .is_some_and(|v| !v.is_finite() || !(0.0..=100.0).contains(&v))
            || window
                .window_minutes
                .is_some_and(|v| v <= 0 || v > 9_007_199_254_740_991)
            || window
                .resets_at_ms
                .is_some_and(|v| !(0..=8_640_000_000_000_000).contains(&v))
        {
            return Err(StorageError::InvalidState);
        }
    }
    Ok(())
}
