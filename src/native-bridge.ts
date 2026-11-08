import {invoke} from "@tauri-apps/api/core";
import {listen} from "@tauri-apps/api/event";
import type {MetricView} from "./metrics-types.ts";
import type {PricingView,MoneyEstimate} from "./pricing-types.ts";
import type {Bridge,DashboardState} from "./types.ts";
import type {HistorySettings,HistoryView} from "./history-types.ts";
import type {ClaudeBridgePlan,ClaudeBridgeStatus} from "./claude-types.ts";
import type {AlertSettingsView} from "./alerts-types.ts";
import type {MonitorSettingsView} from "./monitor-types.ts";
import type {AppConfiguration,DisplaySettings,MaintenanceStatus} from "./configuration-types.ts";
import type {AdapterCatalog} from "./adapter-catalog-types.ts";
import type {DiagnosticsStatus} from "./diagnostics-types.ts";
export function nativeBridge():Bridge {
    return {
        preview:false,
        // Native wake stays unavailable under the user's retry/isolation policy.
        // The backend guards remain in place; synthetic preview can exercise UI.
        getState:()=>invoke<DashboardState>("get_dashboard_state"),
        refresh:()=>invoke<DashboardState>("refresh_agents_command"),
        subscribe:async listener=>listen<DashboardState>("agent-state-updated",event=>listener(event.payload)),
        subscribeSettings:async listener=>listen<null>("settings-requested",()=>listener()),
        takeSettingsRequest:()=>invoke<boolean>("take_settings_request"),
        exportConfiguration:()=>invoke<AppConfiguration>("export_configuration"),
        importConfiguration:(configuration,confirmed)=>invoke<AppConfiguration>("import_configuration",{configuration,confirmed}),
        getDisplaySettings:()=>invoke<DisplaySettings>("get_display_settings"),
        setDisplaySettings:settings=>invoke<DisplaySettings>("set_display_settings",{settings}),
        getStorageMaintenance:()=>invoke<MaintenanceStatus>("get_storage_maintenance"),
        getDiagnosticsStatus:()=>invoke<DiagnosticsStatus>("get_diagnostics_status"),
        getAdapterCatalog:()=>invoke<AdapterCatalog>("get_adapter_catalog"),
        getHistory:rangeHours=>invoke<HistoryView>("get_history",{rangeHours}),
        getHistorySettings:()=>invoke<HistorySettings>("get_history_settings"),
        setHistoryRetention:retentionDays=>invoke<HistorySettings>("set_history_retention",{retentionDays}),
        clearHistory:confirmed=>invoke<void>("clear_history",{confirmed}),
        getClaudeBridgeStatus:()=>invoke<ClaudeBridgeStatus>("get_claude_bridge_status"),
        prepareClaudeBridge:()=>invoke<ClaudeBridgePlan>("prepare_claude_bridge"),
        applyClaudeBridge:(confirmationId,confirmed)=>invoke<ClaudeBridgeStatus>("apply_claude_bridge",{confirmationId,confirmed}),
        restoreClaudeBridge:confirmed=>invoke<ClaudeBridgeStatus>("restore_claude_bridge",{confirmed}),
        getAlertSettings:()=>invoke<AlertSettingsView>("get_alert_settings"),
        setAlertSettings:settings=>invoke<AlertSettingsView>("set_alert_settings",{settings}),
        requestAlertPermission:()=>invoke<AlertSettingsView>("request_alert_permission"),
        sendTestNotification:()=>invoke<void>("send_test_notification"),
        getMonitorSettings:()=>invoke<MonitorSettingsView>("get_monitor_settings"),
        setMonitorSettings:settings=>invoke<MonitorSettingsView>("set_monitor_settings",{settings}),
        getMetrics:rangeDays=>invoke<MetricView>("get_metrics",{rangeDays}),
        setMetricSettings:settings=>invoke<MetricView>("set_metric_settings",{settings}),
        clearMetricHistory:confirmed=>invoke<void>("clear_metric_history",{confirmed}),
        getPricing:()=>invoke<PricingView>("get_pricing"),
        setPriceOverride:price=>invoke<PricingView>("set_price_override",{price}),
        removePriceOverride:modelId=>invoke<PricingView>("remove_price_override",{modelId}),
        estimateCost:(modelId,tokens,context)=>invoke<MoneyEstimate>("estimate_cost",{modelId,tokens,context})
    };
}
