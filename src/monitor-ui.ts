import type { MonitorSettingsView } from "./monitor-types.ts";
export interface MonitorUiState {
    preview: boolean;
    available: boolean;
    canSave: boolean;
    view: MonitorSettingsView | null;
    mode: "" | "running" | "paused";
    interval: string;
    loading: boolean;
    saving: boolean;
    error: string | null;
    message: string | null;
}
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] { const result = document.createElement(tag); result.textContent = text; if (className)
    result.className = className; return result; }
export function renderMonitor(state: MonitorUiState): HTMLElement {
    const section = node("section", "", "monitor-section");
    section.setAttribute("aria-labelledby", "monitor-title");
    const title = node("h2", "Automatic monitoring");
    title.id = "monitor-title";
    section.append(title, node("p", "Choose how often Jentrics reads agent readiness and saved or reported quotas, including while the dashboard is closed. These checks send no model requests.", "quota-meta"));
    if (state.preview)
        section.append(node("p", "Synthetic preview only. These settings stay in preview memory; no automatic checks run.", "quota-meta"));
    if (!state.available) {
        section.append(node("p", "Monitoring settings are unavailable in this connection.", "quota-meta"));
        return section;
    }
    const view = state.view;
    const status = node("p", state.loading ? "Loading monitoring settings…" : view?.settings?.paused ? "Automatic monitoring paused. Refresh now remains available." : view?.settings ? "Automatic monitoring enabled." : "Automatic checks paused: saved monitoring settings are unknown. Refresh now remains available.", "quota-meta");
    status.setAttribute("role", "status");
    status.setAttribute("aria-live", "polite");
    section.append(status);
    if(view?.runtimeReason==="noQualifiedProviders")section.append(node("p","Automatic CLI checks are suspended: no qualified readiness provider is available. Refresh now can discover a supported installation and resume scheduling. Capture and opt-in telemetry remain independent.","error-notice"));
    if (view?.error)
        section.append(node("p", view.error === "invalidSettings" ? "Saved monitoring settings are invalid. Automatic checks remain paused until you save valid choices." : "Saved monitoring settings could not be read. Automatic checks remain paused until you save valid choices.", "error-notice"));
    const controls = node("div", "", "history-controls");
    for (const [id, label, choices, value] of [["monitor-mode", "Automatic checks", [["", "Choose monitoring state"], ["running", "Running"], ["paused", "Paused"]], state.mode], ["monitor-interval", "Check interval", [["", "Choose interval"], ["5", "5 minutes"], ["10", "10 minutes"], ["15", "15 minutes"], ["30", "30 minutes"]], state.interval]] as const) {
        const wrapper = node("label", label, "history-field");
        const select = node("select");
        select.id = id;
        select.disabled = state.saving || state.loading;
        for (const [key, text] of choices) {
            if (key === "" && value !== "")
                continue;
            const option = node("option", text);
            option.value = key;
            option.selected = key === value;
            select.append(option);
        }
        wrapper.append(select);
        controls.append(wrapper);
    }
    for (const [id, label, disabled] of [["monitor-save", state.saving ? "Saving…" : "Save monitoring", !state.canSave || state.saving || state.loading], ["monitor-reload", "Reload monitoring settings", state.saving || state.loading]] as const) {
        const button = node("button", label, "wake-button");
        button.id = id;
        button.type = "button";
        button.disabled = disabled;
        controls.append(button);
    }
    section.append(controls, node("p", "Pause lets a check already in progress finish. Resume requests one fresh check. Missed checks are not replayed. Repeated collection or quota failures increase the interval up to 30 minutes; successful automatic checks restore your chosen interval.", "quota-meta"));
    if (view?.effectiveIntervalMinutes !== null && view?.effectiveIntervalMinutes !== undefined && view.settings) {
        section.append(node("p", view.effectiveIntervalMinutes > view.settings.intervalMinutes ? `Current retry interval: ${view.effectiveIntervalMinutes} minutes after collection or quota failures.` : `Current scheduled interval: ${view.effectiveIntervalMinutes} minutes.`, "quota-meta"));
    }
    if (view?.nextCheckMs !== null && view?.nextCheckMs !== undefined)
        section.append(node("p", view.nextCheckMs > Date.now() ? `Next scheduled check: ${new Date(view.nextCheckMs).toLocaleString()}` : "Scheduled check time passed · awaiting monitor update.", "quota-meta"));
    if (view?.lastClockChangeMs !== null && view?.lastClockChangeMs !== undefined)
        section.append(node("p", `Clock or scheduling change detected: ${new Date(view.lastClockChangeMs).toLocaleString()}. Quota freshness still depends on the provider's observation time.`, "quota-meta"));
    if (!state.canSave)
        section.append(node("p", "Changing monitoring settings is unavailable in this connection.", "quota-meta"));
    for (const [text, className] of [[state.error, "error-notice"], [state.message, "quota-meta"]])
        if (text) {
            const message = node("p", text, className!);
            message.setAttribute("role", "status");
            message.setAttribute("aria-live", "polite");
            section.append(message);
        }
    return section;
}
