use super::{QuotaEffect, WakeReason};
use crate::model::{AgentId, AgentState, AuthState, Availability, DashboardState};

fn fresh(state: &DashboardState, id: AgentId, now: i64) -> Result<&AgentState, WakeReason> {
    let agent = state
        .agents
        .iter()
        .find(|a| a.agent_id == id)
        .ok_or(WakeReason::NotInstalled)?;
    match agent.availability {
        Availability::NotFound => return Err(WakeReason::NotInstalled),
        Availability::Unsupported => return Err(WakeReason::Unsupported),
        Availability::ProbeFailed => return Err(WakeReason::ProviderUnavailable),
        Availability::Detected => {}
    }
    if state.collection_error.is_some()
        || state.last_refresh_ms.is_none()
        || !(0..900_000).contains(&(now - agent.observed_ms))
    {
        return Err(WakeReason::ProviderUnavailable);
    }
    if agent.auth_state != AuthState::Ready {
        return Err(WakeReason::SignInRequired);
    }
    Ok(agent)
}
pub fn check(state: &DashboardState, id: AgentId, now: i64) -> Result<bool, WakeReason> {
    let agent = fresh(state, id, now)?;
    let Some(quota) = agent
        .quota
        .as_ref()
        .filter(|q| q.diagnostic_code.is_none() && (0..900_000).contains(&(now - q.observed_ms)))
    else {
        return Ok(true);
    };
    if quota
        .windows
        .iter()
        .any(|w| w.window_minutes == Some(300) && w.resets_at_ms.is_some_and(|reset| reset > now))
    {
        return Err(WakeReason::ActiveWindow);
    }
    if !quota.windows.is_empty()
        && quota
            .windows
            .iter()
            .all(|w| w.window_minutes == Some(10080))
    {
        return Err(WakeReason::WeeklyOnly);
    }
    Ok(!quota
        .windows
        .iter()
        .any(|w| w.window_minutes == Some(300) && w.resets_at_ms.is_some()))
}
pub fn quota_effect(
    before: &DashboardState,
    after: &DashboardState,
    id: AgentId,
    now: i64,
) -> QuotaEffect {
    let Ok(agent) = fresh(after, id, now) else {
        return QuotaEffect::Unverified;
    };
    let Some(quota) = agent
        .quota
        .as_ref()
        .filter(|q| q.diagnostic_code.is_none() && (0..900_000).contains(&(now - q.observed_ms)))
    else {
        return QuotaEffect::Unverified;
    };
    let old = before
        .agents
        .iter()
        .find(|a| a.agent_id == id)
        .and_then(|a| a.quota.as_ref());
    let mut reported = false;
    for window in quota
        .windows
        .iter()
        .filter(|w| w.window_minutes == Some(300))
    {
        let Some(reset) = window.resets_at_ms else {
            continue;
        };
        reported = true;
        let previous = old
            .and_then(|q| q.windows.iter().find(|w| w.bucket_id == window.bucket_id))
            .and_then(|w| w.resets_at_ms);
        if reset > now && previous.is_none_or(|previous| previous <= now && reset > previous) {
            return QuotaEffect::NewWindowObserved;
        }
    }
    if reported {
        QuotaEffect::Unchanged
    } else {
        QuotaEffect::Unverified
    }
}
