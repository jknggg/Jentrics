//! Fixed tray text derived only from typed readiness and reported reset values.
use crate::{
    model::{AgentId, AuthState, Availability, DashboardState},
    state::{MonitorRuntimeReason, MonitorSettings, MonitorSettingsView},
};
pub struct TrayView {
    pub claude_status: String,
    pub claude_reset: String,
    pub codex_status: String,
    pub codex_reset: String,
    pub pause_label: &'static str,
    pub pause_enabled: bool,
    pub tooltip: String,
}
pub fn pause_action(monitor: &MonitorSettingsView) -> Option<MonitorSettings> {
    let mut settings = monitor.settings.clone()?;
    settings.paused = if monitor.runtime_reason == Some(MonitorRuntimeReason::NoQualifiedProviders)
    {
        false
    } else {
        !settings.paused
    };
    Some(settings)
}
pub fn view(snapshot: &DashboardState, monitor: &MonitorSettingsView, now_ms: i64) -> TrayView {
    let provider = |id, name| {
        let Some(agent) = snapshot.agents.iter().find(|agent| agent.agent_id == id) else {
            return (
                format!("{name}: not checked"),
                format!("{name}: reset unknown"),
            );
        };
        let status = match agent.availability {
            Availability::NotFound => "not installed",
            Availability::Unsupported => "unsupported version",
            Availability::ProbeFailed => "check failed",
            Availability::Detected => match agent.auth_state {
                AuthState::Ready => "ready",
                AuthState::NotAuthenticated => "sign-in required",
                AuthState::Unknown => "readiness unknown",
            },
        };
        let reset = agent.quota.as_ref().and_then(|quota| {
            quota
                .windows
                .iter()
                .filter_map(|window| window.resets_at_ms)
                .min()
        });
        let reset = match reset {
            Some(reset) if reset <= now_ms => {
                format!("{name}: reported reset passed; awaiting fresh quota")
            }
            Some(reset) => format!(
                "{name}: next reported reset in {}m (at update)",
                (reset.saturating_sub(now_ms).saturating_add(59999)) / 60000
            ),
            None => format!("{name}: reset unknown"),
        };
        (format!("{name}: {status}"), reset)
    };
    let (claude_status, claude_reset) = provider(AgentId::Claude, "Claude");
    let (codex_status, codex_reset) = provider(AgentId::Codex, "Codex");
    let suspended = matches!(
        monitor.runtime_reason,
        Some(MonitorRuntimeReason::Paused | MonitorRuntimeReason::NoQualifiedProviders)
    );
    let schedule = match monitor.runtime_reason {
        Some(MonitorRuntimeReason::Paused) => "monitoring paused".to_owned(),
        Some(MonitorRuntimeReason::NoQualifiedProviders) => {
            "monitoring suspended: no supported CLI".to_owned()
        }
        Some(MonitorRuntimeReason::SettingsUnavailable) => {
            "monitor settings unavailable".to_owned()
        }
        None => monitor
            .settings
            .as_ref()
            .map_or("monitor settings unavailable".to_owned(), |settings| {
                format!("monitoring every {}m", settings.interval_minutes)
            }),
    };
    let tooltip = format!("Jentrics · {schedule}\n{claude_status}\n{codex_status}");
    TrayView {
        claude_status,
        claude_reset,
        codex_status,
        codex_reset,
        pause_label: if suspended {
            "Resume monitoring"
        } else {
            "Pause monitoring"
        },
        pause_enabled: monitor.settings.is_some(),
        tooltip,
    }
}
