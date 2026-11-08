use crate::{model::DashboardState, state::AppState};
use std::sync::Arc;
#[tauri::command]
pub fn get_dashboard_state(state: tauri::State<'_, Arc<AppState>>) -> DashboardState {
    state.snapshot()
}
#[tauri::command]
pub fn get_history(
    state: tauri::State<'_, Arc<AppState>>,
    range_hours: u16,
) -> Result<crate::history::HistoryView, crate::model::DiagnosticCode> {
    state.history(range_hours)
}
#[tauri::command]
pub fn get_history_settings(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::history::HistorySettings, crate::model::DiagnosticCode> {
    state.history_settings()
}
#[tauri::command]
pub fn set_history_retention(
    state: tauri::State<'_, Arc<AppState>>,
    retention_days: u16,
) -> Result<crate::history::HistorySettings, crate::model::DiagnosticCode> {
    state.set_history_retention(retention_days)
}
#[tauri::command]
pub fn clear_history(
    state: tauri::State<'_, Arc<AppState>>,
    confirmed: bool,
) -> Result<(), crate::model::DiagnosticCode> {
    state.clear_history(confirmed)
}
#[tauri::command]
pub async fn refresh_agents_command(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DashboardState, crate::model::DiagnosticCode> {
    let state = state.inner().clone();
    Ok(state.refresh().await)
}

#[tauri::command]
pub async fn prepare_wake(
    state: tauri::State<'_, Arc<AppState>>,
    agent_id: crate::model::AgentId,
) -> Result<crate::wake::WakeOffer, crate::wake::WakeReason> {
    let state = state.inner().clone();
    Ok(state.prepare_wake(agent_id).await)
}
#[tauri::command]
pub async fn confirm_wake(
    state: tauri::State<'_, Arc<AppState>>,
    agent_id: crate::model::AgentId,
    confirmation_id: String,
    confirmed: bool,
) -> Result<crate::wake::WakeOutcome, crate::wake::WakeReason> {
    let state = state.inner().clone();
    Ok(state
        .confirm_wake(agent_id, confirmation_id, confirmed)
        .await)
}
#[tauri::command]
pub fn get_claude_bridge_status(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::claude_bridge::control::BridgeStatus, crate::claude_bridge::control::BridgeIssue>
{
    state.claude_bridge_status()
}
#[tauri::command]
pub fn prepare_claude_bridge(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::claude_bridge::control::BridgeReview, crate::claude_bridge::control::BridgeIssue>
{
    state.prepare_claude_bridge()
}
#[tauri::command]
pub fn apply_claude_bridge(
    state: tauri::State<'_, Arc<AppState>>,
    confirmation_id: String,
    confirmed: bool,
) -> Result<crate::claude_bridge::control::BridgeStatus, crate::claude_bridge::control::BridgeIssue>
{
    state.apply_claude_bridge(&confirmation_id, confirmed)
}
#[tauri::command]
pub fn restore_claude_bridge(
    state: tauri::State<'_, Arc<AppState>>,
    confirmed: bool,
) -> Result<crate::claude_bridge::control::BridgeStatus, crate::claude_bridge::control::BridgeIssue>
{
    state.restore_claude_bridge(confirmed)
}
#[tauri::command]
pub fn get_metrics(
    state: tauri::State<'_, Arc<AppState>>,
    range_days: u16,
) -> Result<crate::state::MetricView, crate::model::DiagnosticCode> {
    state.metrics_view(range_days)
}
#[tauri::command]
pub fn set_metric_settings(
    state: tauri::State<'_, Arc<AppState>>,
    settings: crate::metrics::MetricSettings,
) -> Result<crate::state::MetricView, crate::model::DiagnosticCode> {
    state.inner().save_metric_settings(settings)
}
#[tauri::command]
pub fn clear_metric_history(
    state: tauri::State<'_, Arc<AppState>>,
    confirmed: bool,
) -> Result<(), crate::model::DiagnosticCode> {
    state.clear_metric_history(confirmed)
}
#[tauri::command]
pub fn get_pricing(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::state::PricingView, crate::model::DiagnosticCode> {
    state.pricing_view()
}
#[tauri::command]
pub fn set_price_override(
    state: tauri::State<'_, Arc<AppState>>,
    price: crate::pricing::ManualPriceOverride,
) -> Result<crate::state::PricingView, crate::model::DiagnosticCode> {
    state.set_price_override(price)
}
#[tauri::command]
pub fn remove_price_override(
    state: tauri::State<'_, Arc<AppState>>,
    model_id: String,
) -> Result<crate::state::PricingView, crate::model::DiagnosticCode> {
    state.remove_price_override(&model_id)
}
#[tauri::command]
pub fn estimate_cost(
    state: tauri::State<'_, Arc<AppState>>,
    model_id: String,
    tokens: crate::pricing::BillingTokens,
    context: crate::pricing::PricingContext,
) -> Result<crate::pricing::MoneyEstimate, crate::model::DiagnosticCode> {
    state.estimate_cost(&model_id, &tokens, &context)
}
#[tauri::command]
pub fn get_monitor_settings(
    state: tauri::State<'_, Arc<AppState>>,
) -> crate::state::MonitorSettingsView {
    state.monitor_settings()
}
#[tauri::command]
pub fn set_monitor_settings(
    state: tauri::State<'_, Arc<AppState>>,
    settings: crate::state::MonitorSettings,
) -> Result<crate::state::MonitorSettingsView, crate::state::MonitorError> {
    state.save_monitor_settings(settings)
}
