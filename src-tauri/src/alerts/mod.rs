use crate::model::{AgentId, AgentState};
use serde::{Deserialize, Serialize};

const FRESH_MS: i64 = 900_000;
const NEAR_RESET_MS: i64 = 600_000;
const MAX_CLOCK_MS: i64 = 8_640_000_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuietHours {
    pub start_minutes: u16,
    pub end_minutes: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AlertSettings {
    pub enabled: bool,
    pub threshold80: bool,
    pub threshold95: bool,
    pub near_reset: bool,
    pub expected_reset: bool,
    pub restored: bool,
    pub errors: bool,
    pub quiet_hours: Option<QuietHours>,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold80: true,
            threshold95: true,
            near_reset: true,
            expected_reset: true,
            restored: true,
            errors: true,
            quiet_hours: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertKind {
    Threshold80,
    Threshold95,
    NearReset,
    ResetExpected,
    QuotaRestored,
    QuotaError,
    MonitoringError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AlertKey {
    pub agent_id: Option<AgentId>,
    pub bucket_id: String,
    pub resets_at_ms: Option<i64>,
    pub kind: AlertKind,
    /// Error advisories use an episode epoch; continued failures do not alert.
    pub epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertEvent {
    pub key: AlertKey,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertPolicyError {
    InvalidSettings,
    InvalidClock,
    InvalidLocalMinute,
}

pub fn validate_settings(settings: &AlertSettings) -> Result<(), AlertPolicyError> {
    if settings.quiet_hours.as_ref().is_some_and(|quiet| {
        quiet.start_minutes >= 1440
            || quiet.end_minutes >= 1440
            || quiet.start_minutes == quiet.end_minutes
    }) {
        return Err(AlertPolicyError::InvalidSettings);
    }
    Ok(())
}

fn alerts_enabled(
    settings: &AlertSettings,
    now_ms: i64,
    local_minute: u16,
) -> Result<bool, AlertPolicyError> {
    validate_settings(settings)?;
    if !(0..=MAX_CLOCK_MS).contains(&now_ms) {
        return Err(AlertPolicyError::InvalidClock);
    }
    if local_minute >= 1440 {
        return Err(AlertPolicyError::InvalidLocalMinute);
    }
    let quiet = settings.quiet_hours.as_ref().is_some_and(|hours| {
        if hours.start_minutes < hours.end_minutes {
            local_minute >= hours.start_minutes && local_minute < hours.end_minutes
        } else {
            local_minute >= hours.start_minutes || local_minute < hours.end_minutes
        }
    });
    Ok(settings.enabled && !quiet)
}

fn fresh(observed_ms: i64, now_ms: i64) -> bool {
    observed_ms >= 0 && observed_ms <= now_ms && now_ms - observed_ms < FRESH_MS
}

fn quota_error(agent: &AgentState) -> bool {
    agent.monitoring_error()
}

/// Pure policy only. The caller persists keys and performs notification delivery.
/// Quiet hours use the caller's current local clock minute, never inferred UTC.
pub fn evaluate_alerts(
    settings: &AlertSettings,
    current: &[AgentState],
    previous: &[AgentState],
    seen: &[AlertKey],
    now_ms: i64,
    local_minute: u16,
) -> Result<Vec<AlertEvent>, AlertPolicyError> {
    if !alerts_enabled(settings, now_ms, local_minute)? {
        return Ok(Vec::new());
    }
    let mut events = Vec::<AlertEvent>::new();
    for agent in current {
        let prior = previous
            .iter()
            .find(|prior| prior.agent_id == agent.agent_id);
        let provider = match agent.agent_id {
            AgentId::Codex => "Codex",
            AgentId::Claude => "Claude Code",
        };
        if quota_error(agent) {
            let error_observed = agent
                .quota
                .as_ref()
                .filter(|quota| quota.diagnostic_code.is_some())
                .map_or(agent.observed_ms, |quota| quota.observed_ms);
            if !settings.errors || !fresh(error_observed, now_ms) || prior.is_some_and(quota_error)
            {
                continue;
            }
            let buckets: Vec<_> = agent
                .quota
                .as_ref()
                .map(|quota| {
                    quota
                        .windows
                        .iter()
                        .map(|window| (window.bucket_id.as_str(), window.resets_at_ms))
                        .collect()
                })
                .unwrap_or_default();
            let buckets = if buckets.is_empty() {
                vec![("__quota_check", None)]
            } else {
                buckets
            };
            for (bucket, reset) in buckets {
                push_event(&mut events, seen, AlertKey { agent_id: Some(agent.agent_id), bucket_id: bucket.into(), resets_at_ms: reset, kind: AlertKind::QuotaError, epoch_ms: Some(error_observed) }, format!("{provider} quota check unavailable"), "The latest quota check failed. Saved values are unverified; this does not establish your remaining capacity.".into());
            }
            continue;
        }
        let Some(quota) = &agent.quota else { continue };
        if !fresh(quota.observed_ms, now_ms) {
            continue;
        }
        for window in &quota.windows {
            let (Some(used), Some(reset), Some(minutes)) = (
                window.used_percent,
                window.resets_at_ms,
                window.window_minutes,
            ) else {
                continue;
            };
            if !fresh(window.observed_ms, now_ms)
                || !used.is_finite()
                || !(0.0..=100.0).contains(&used)
                || !(0..=MAX_CLOCK_MS).contains(&reset)
                || minutes <= 0
            {
                continue;
            }
            let key = |kind| AlertKey {
                agent_id: Some(agent.agent_id),
                bucket_id: window.bucket_id.clone(),
                resets_at_ms: Some(reset),
                kind,
                epoch_ms: None,
            };
            let mut emit = |kind, title: &str, message: String| {
                push_event(
                    &mut events,
                    seen,
                    key(kind),
                    format!("{provider}: {title}"),
                    message,
                )
            };
            if settings.threshold95 && used >= 95.0 {
                emit(
                    AlertKind::Threshold95,
                    "95% quota threshold",
                    format!(
                        "{}: last reported usage was {used}% used.",
                        window.bucket_id
                    ),
                );
            } else if settings.threshold80
                && (80.0..95.0).contains(&used)
                && !seen.contains(&key(AlertKind::Threshold95))
            {
                emit(
                    AlertKind::Threshold80,
                    "80% quota threshold",
                    format!(
                        "{}: last reported usage was {used}% used.",
                        window.bucket_id
                    ),
                );
            }
            if settings.near_reset && reset > now_ms && reset - now_ms <= NEAR_RESET_MS {
                emit(AlertKind::NearReset, "Expected reset is near", format!("{}: the reported reset is within 10 minutes. Its timing and future capacity remain unverified.", window.bucket_id));
            }
            if settings.expected_reset && reset <= now_ms {
                emit(AlertKind::ResetExpected, "Expected reset time passed", format!("{}: the reported reset time passed; verify with a fresh provider observation. Capacity has not been confirmed.", window.bucket_id));
            }
            if settings.restored && used < 95.0 && reset > now_ms {
                let prior_window = prior
                    .filter(|prior| !quota_error(prior))
                    .and_then(|prior| prior.quota.as_ref())
                    .and_then(|quota| {
                        quota
                            .windows
                            .iter()
                            .find(|prior| prior.bucket_id == window.bucket_id)
                    });
                let restored = prior_window.is_some_and(|prior| {
                    prior.window_minutes == Some(minutes)
                        && prior
                            .used_percent
                            .is_some_and(|used| used.is_finite() && (95.0..=100.0).contains(&used))
                        && prior.resets_at_ms.is_some_and(|old_reset| {
                            old_reset >= 0
                                && old_reset < reset
                                && old_reset <= now_ms
                                && prior.observed_ms >= 0
                                && prior.observed_ms < old_reset
                                && window.observed_ms >= old_reset
                        })
                });
                if restored {
                    emit(AlertKind::QuotaRestored, "Quota room reported after reset", format!("{}: fresh reported quota shows room after the previous expected reset ({used}% used). This is an observation, not a guarantee of request capacity.", window.bucket_id));
                }
            }
        }
    }
    Ok(events)
}

fn push_event(
    events: &mut Vec<AlertEvent>,
    seen: &[AlertKey],
    key: AlertKey,
    title: String,
    message: String,
) {
    if !seen.contains(&key) && !events.iter().any(|event| event.key == key) {
        events.push(AlertEvent {
            key,
            title,
            message,
        });
    }
}

/// Global collection failures have app scope, independent of discovered agents.
/// Quiet hours suppress new episodes without queuing a catch-up notification.
pub fn evaluate_monitor_error(
    settings: &AlertSettings,
    failed: bool,
    previous_failed: bool,
    seen: &[AlertKey],
    now_ms: i64,
    local_minute: u16,
) -> Result<Option<AlertEvent>, AlertPolicyError> {
    if !alerts_enabled(settings, now_ms, local_minute)?
        || !settings.errors
        || !failed
        || previous_failed
    {
        return Ok(None);
    }
    let key = AlertKey {
        agent_id: None,
        bucket_id: "__monitoring".into(),
        resets_at_ms: None,
        kind: AlertKind::MonitoringError,
        epoch_ms: Some(now_ms),
    };
    if seen.contains(&key) {
        return Ok(None);
    }
    Ok(Some(AlertEvent {
        key,
        title: "Jentrics monitoring unavailable".into(),
        message: "The latest monitoring check failed. Provider observations are unavailable or unverified; this does not establish remaining capacity.".into(),
    }))
}
mod windows;
pub use windows::{windows_state, WindowsNotificationState};
