import type { AppConfiguration, MaintenanceStatus } from "./configuration-types.ts";
import type { DiagnosticsStatus } from "./diagnostics-types.ts";
import { renderDiagnostics } from "./diagnostics-ui.ts";
export interface ConfigurationUiState {
    preview: boolean;
    diagnostics: DiagnosticsStatus | null;
    diagnosticsError: boolean;
    available: boolean;
    canImport: boolean;
    busy: boolean;
    reading: boolean;
    saved: AppConfiguration | null;
    error: string | null;
    message: string | null;
    maintenance: MaintenanceStatus | null;
    maintenanceError: boolean;
}
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] { const result = document.createElement(tag); result.textContent = text; if (className)
    result.className = className; return result; }
function summary(configuration: AppConfiguration): HTMLElement { const list = node("ul", "", "configuration-summary"); for (const text of [`Monitoring: ${configuration.monitoring.paused ? "Paused" : "Running"}; every ${configuration.monitoring.intervalMinutes} minutes.`, `Automatic alerts: ${configuration.alerts.enabled ? "Enabled" : "Disabled"}. No permission prompt is requested by import.`, `Local telemetry collector: ${configuration.telemetry.enabled ? "Enabled" : "Disabled"}. Import can start or stop the local receiver.`, `Retention: quota ${configuration.quotaRetentionDays} days; numeric exports ${configuration.telemetry.eventRetentionDays} days; daily aggregates ${configuration.telemetry.dailyRetentionDays} days.`, `Primary buckets: Claude ${configuration.display.primaryBuckets.claude === null ? "none" : configuration.display.primaryBuckets.claude}; Codex ${configuration.display.primaryBuckets.codex === null ? "none" : configuration.display.primaryBuckets.codex}.`, `Manual price overrides: ${configuration.priceOverrides.length}. Reference metadata includes user-entered model IDs and source URLs.`])
    list.append(node("li", text)); for (const price of configuration.priceOverrides)
    list.append(node("li", `${price.modelId} · ${price.retrievedOn} · ${price.sourceUrls.length} source reference(s)`)); return list; }
export function renderConfiguration(state: ConfigurationUiState): HTMLElement { const section = node("section", "", "configuration-section"); section.setAttribute("aria-labelledby", "configuration-title"); const title = node("h2", "App configuration"); title.id = "configuration-title"; title.tabIndex = -1; section.append(title, node("p", "Export or replace app-owned settings. Export includes manually entered price-reference metadata. Provider configuration, credentials, readiness/account snapshots, history, backup files and diagnostics are excluded.", "quota-meta")); if (state.preview)
    section.append(node("p", "Synthetic preview only. Configuration and maintenance describe preview memory; no native storage or receiver is changed.", "quota-meta")); if (!state.available) {
    section.append(node("p", "Configuration export and import are unavailable in this connection.", "quota-meta"));
    return section;
} const actions = node("div", "", "history-controls"); for (const [id, label] of [["configuration-export", "Download configuration"], ["configuration-reload", "Reload saved settings"]]) {
    const button = node("button", label, "wake-button");
    button.id = id!;
    button.type = "button";
    button.disabled = state.busy;
    actions.append(button);
} const label = node("label", "Review configuration JSON (maximum 128 KiB)", "history-field"); const file = node("input"); file.id = "configuration-file"; file.type = "file"; file.accept = ".json,application/json"; file.disabled = state.busy || !state.canImport; label.append(file); actions.append(label); section.append(actions); if (state.reading)
    section.append(node("p", "Reading configuration for review…", "quota-meta")); if (state.saved) {
    const details = node("details");
    details.append(node("summary", "Last loaded saved configuration"), summary(state.saved));
    section.append(details);
} if (!state.canImport)
    section.append(node("p", "Import is unavailable in this connection.", "quota-meta")); for (const [text, className] of [[state.error, "error-notice"], [state.message, "quota-meta"]])
    if (text) {
        const message = node("p", text, className!);
        message.setAttribute("role", "status");
        message.setAttribute("aria-live", "polite");
        section.append(message);
    } section.append(renderMaintenance(state.maintenance, state.maintenanceError), renderDiagnostics(state.diagnostics, state.diagnosticsError)); return section; }
export function renderMaintenance(status: MaintenanceStatus | null, error: boolean): HTMLElement { const box = node("div", "", "maintenance-status"); box.append(node("h3", "Local storage maintenance")); if (error || !status) {
    box.append(node("p", "Storage maintenance status is unavailable in this connection or could not be read.", "quota-meta"));
    return box;
} const committed = status.dataCommitted ? "The data changes were committed." : "No data change was confirmed by this operation."; let text: string; switch (status.outcome) {
    case "neverRun":
        text = "Storage maintenance has not run yet.";
        break;
    case "notNeeded":
        text = `Compaction was not needed after the last change. ${committed}`;
        break;
    case "compacted":
        text = `Storage was compacted. ${committed} ${status.reclaimedBytes === null ? "Reclaimed size is unknown." : `${status.reclaimedBytes} bytes reclaimed.`}`;
        break;
    case "deferredTooLarge":
        text = `Compaction was deferred to avoid an expensive rewrite. ${committed}`;
        break;
    case "failed":
        text = `Storage maintenance failed (${status.error === null ? "detail unavailable" : status.error}). ${committed} Compaction is not confirmed; reload saved data to verify its state.`;
        break;
} const notice = node("p", text, status.outcome === "failed" || status.outcome === "deferredTooLarge" ? "error-notice" : "quota-meta"); notice.setAttribute("role", "status"); box.append(notice); return box; }
export function renderConfigurationImport(configuration: AppConfiguration, preview: boolean, confirm: (activation: boolean) => void, cancel: () => void): HTMLElement { const overlay = node("div", "", "wake-overlay"); const dialog = node("section", "", "wake-dialog"); dialog.setAttribute("role", "dialog"); dialog.setAttribute("aria-modal", "true"); dialog.setAttribute("aria-labelledby", "configuration-import-title"); const title = node("h2", "Replace app settings?"); title.id = "configuration-import-title"; dialog.append(title, node("p", "This replaces all listed app settings and manual price overrides. Shorter retention may remove older observations. Provider configuration and credentials remain unchanged."), summary(configuration)); const activation = configuration.alerts.enabled || configuration.telemetry.enabled; const checkbox = node("input"); checkbox.id = "configuration-activate"; checkbox.type = "checkbox"; if (activation) {
    const label = node("label", "", "alerts-check");
    label.append(checkbox, document.createTextNode("I understand this enables automatic alerts or the local telemetry receiver on this computer."));
    dialog.append(label);
} if (preview)
    dialog.append(node("p", "Synthetic preview only. This replaces preview memory; no provider or OS settings are changed.", "wake-warning")); const actions = node("div", "", "wake-dialog-actions"); const no = node("button", "Cancel", "wake-button"); no.id = "configuration-cancel"; no.type = "button"; no.addEventListener("click", cancel); const yes = node("button", "Replace app settings", "refresh-button"); yes.id = "configuration-confirm"; yes.type = "button"; yes.disabled = activation; checkbox.addEventListener("change", () => { yes.disabled = !checkbox.checked; }); yes.addEventListener("click", () => { confirm(!activation || checkbox.checked); }); actions.append(no, yes); dialog.append(actions); overlay.append(dialog); overlay.addEventListener("keydown", event => { if (event.key === "Escape") {
    event.preventDefault();
    cancel();
} if (event.key === "Tab") {
    event.preventDefault();
    const controls = activation ? [no, checkbox, yes] : [no, yes];
    const active = controls.indexOf(document.activeElement as HTMLButtonElement);
    const next = (active + (event.shiftKey ? -1 : 1) + controls.length) % controls.length;
    const target = controls[next]!;
    if (target.disabled)
        no.focus();
    else
        target.focus();
} }); return overlay; }
