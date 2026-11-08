import type { MonitorBridge, MonitorSettingsView } from "./monitor-types.ts";
import { renderMonitor } from "./monitor-ui.ts";
import type { MonitorUiState } from "./monitor-ui.ts";
export function createMonitorController(bridge: MonitorBridge, onChanged: () => void = () => {}) {
    const element = document.createElement("div");
    const state: MonitorUiState = { preview: bridge.preview === true, available: !!bridge.getMonitorSettings, canSave: !!bridge.setMonitorSettings, view: null, mode: "", interval: "", loading: false, saving: false, error: null, message: null };
    let disposed = false;
    let revision = 0;
    let dirty = false;
    function adopt(view: MonitorSettingsView, preserveDraft: boolean) { state.view = view; if (!preserveDraft || !dirty) {
        state.mode = view.settings ? (view.settings.paused ? "paused" : "running") : "";
        state.interval = view.settings ? String(view.settings.intervalMinutes) : "";
        dirty = false;
    } }
    function paint() { if (disposed)
        return; const focusId = element.contains(document.activeElement) ? document.activeElement?.id : undefined; element.replaceChildren(renderMonitor(state)); element.querySelector<HTMLSelectElement>("#monitor-mode")?.addEventListener("change", event => { const value = (event.currentTarget as HTMLSelectElement).value; if (value === "running" || value === "paused") {
        state.mode = value;
        dirty = true;
    } }); element.querySelector<HTMLSelectElement>("#monitor-interval")?.addEventListener("change", event => { state.interval = (event.currentTarget as HTMLSelectElement).value; dirty = true; }); element.querySelector("#monitor-save")?.addEventListener("click", () => { void save(); }); element.querySelector("#monitor-reload")?.addEventListener("click", () => { void load(false); }); if (focusId)
        element.querySelector<HTMLElement>(`#${focusId}`)?.focus(); onChanged(); }
    async function load(preserveDraft = false) { if (disposed || state.saving || !bridge.getMonitorSettings)
        return; const current = ++revision; if (!preserveDraft) {
        state.loading = true;
        state.error = null;
        state.message = null;
        paint();
    } try {
        const view = await bridge.getMonitorSettings();
        if (!disposed && current === revision)
            adopt(view, preserveDraft);
    }
    catch {
        if (!disposed && current === revision) {
            state.view = null;
            state.error = "Monitoring settings could not be loaded. Automatic settings are unknown; reload or save explicit choices.";
            if (!dirty) {
                state.mode = "";
                state.interval = "";
            }
        }
    }
    finally {
        if (!disposed && current === revision) {
            state.loading = false;
            paint();
        }
    } }
    async function save() { if (disposed || state.saving || state.loading || !bridge.setMonitorSettings)
        return; const intervalMinutes = Number(state.interval); if (!state.mode || (intervalMinutes !== 5 && intervalMinutes !== 10 && intervalMinutes !== 15 && intervalMinutes !== 30)) {
        state.error = "Choose a monitoring state and a check interval before saving.";
        paint();
        return;
    } state.saving = true; state.error = null; state.message = null; ++revision; paint(); try {
        const result = await bridge.setMonitorSettings({ paused: state.mode === "paused", intervalMinutes });
        if (disposed)
            return;
        adopt(result, false);
        if (!result.error)
            state.message = "Monitoring settings saved. Refresh now remains available.";
    }
    catch {
        if (!disposed)
            state.error = "Monitoring settings could not be saved. Reload settings to verify the current configuration.";
    }
    finally {
        state.saving = false;
        paint();
        if (!disposed)
            element.querySelector<HTMLElement>("#monitor-save")?.focus();
    } }
    paint();
    return { element, snapshot: () => state.view, start: () => load(false), reload: () => load(true), dispose() { disposed = true; ++revision; } };
}
