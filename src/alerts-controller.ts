import type { AlertSettingsView, AlertsBridge } from "./alerts-types.ts";
import { alertChecks, renderAlerts } from "./alerts-ui.ts";
import type { AlertsUiState } from "./alerts-ui.ts";
function formatMinute(minute: number): string { return `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`; }
function parseMinute(text: string): number | null {
    if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(text))
        return null;
    return Number(text.slice(0, 2)) * 60 + Number(text.slice(3));
}
export function createAlertsController(bridge: AlertsBridge) {
    const element = document.createElement("div");
    const state: AlertsUiState = { preview: bridge.preview === true, available: !!bridge.getAlertSettings, canSave: !!bridge.setAlertSettings, canRequestPermission: !!bridge.requestAlertPermission, canSendTest: !!bridge.sendTestNotification, view: null, draft: null, quietEnabled: false, quietStart: "", quietEnd: "", loading: false, busy: false, error: null, message: null };
    let disposed = false;
    let revision = 0;let dirty=false;
    function adopt(view: AlertSettingsView) { dirty=false;state.view = view; state.draft = { ...view.settings, quietHours: view.settings.quietHours ? { ...view.settings.quietHours } : null }; state.quietEnabled = view.settings.quietHours !== null; state.quietStart = view.settings.quietHours ? formatMinute(view.settings.quietHours.startMinutes) : ""; state.quietEnd = view.settings.quietHours ? formatMinute(view.settings.quietHours.endMinutes) : ""; }
    function paint() {
        if (disposed)
            return;
        const focusId = element.contains(document.activeElement) ? document.activeElement?.id : undefined;
        element.replaceChildren(renderAlerts(state));
        for (const [key] of alertChecks)
            element.querySelector<HTMLInputElement>(`#alerts-${key}`)?.addEventListener("change", event => {
                if (state.draft)
                    {state.draft[key] = (event.currentTarget as HTMLInputElement).checked;dirty=true;}
            });
        element.querySelector<HTMLInputElement>("#alerts-quiet")?.addEventListener("change", event => { state.quietEnabled = (event.currentTarget as HTMLInputElement).checked;dirty=true; paint(); });
        element.querySelector<HTMLInputElement>("#alerts-quiet-start")?.addEventListener("input", event => { state.quietStart = (event.currentTarget as HTMLInputElement).value;dirty=true; });
        element.querySelector<HTMLInputElement>("#alerts-quiet-end")?.addEventListener("input", event => { state.quietEnd = (event.currentTarget as HTMLInputElement).value;dirty=true; });
        element.querySelector("#alerts-save")?.addEventListener("click", () => { void save(); });
        element.querySelector("#alerts-permission")?.addEventListener("click", () => { void permission(); });
        element.querySelector("#alerts-reload")?.addEventListener("click", () => { void load(); });
        if (focusId)
            element.querySelector<HTMLElement>(`#${focusId}`)?.focus();
        element.querySelector("#alerts-test")?.addEventListener("click", () => { void sendTest(); });
    }
    async function load() {
        if (disposed || state.busy || state.loading || !bridge.getAlertSettings)
            return;
        const current = ++revision;
        state.loading = true;
        state.error = null;
        state.message = null;
        paint();
        try {
            const view = await bridge.getAlertSettings();
            if (!disposed && current === revision)
                adopt(view);
        }
        catch {
            if (!disposed && current === revision) {
                state.view = null;
                state.draft = null;
                state.error = "Alert settings could not be loaded. Reload alert settings to try again.";
            }
        }
        finally {
            if (!disposed && current === revision) {
                state.loading = false;
                paint();
            }
        }
    }
    async function save() {
        if (disposed || state.busy || state.loading || !state.draft || !bridge.setAlertSettings)
            return;
        let quietHours = null;
        if (state.quietEnabled) {
            const startMinutes = parseMinute(state.quietStart);
            const endMinutes = parseMinute(state.quietEnd);
            if (startMinutes === null || endMinutes === null) {
                state.error = "Enter both start and end times for quiet hours.";
                paint();
                return;
            }
            if (startMinutes === endMinutes) {
                state.error = "Quiet hours start and end must differ.";
                paint();
                return;
            }
            quietHours = { startMinutes, endMinutes };
        }
        const settings = { ...state.draft, quietHours };
        state.busy = true;
        state.error = null;
        state.message = null;
        ++revision;
        paint();
        try {
            const result = await bridge.setAlertSettings(settings);
            if (disposed)
                return;
            adopt(result);
            if (!result.error)
                state.message = "Alert settings saved. Permission and Windows settings determine whether notifications can appear.";
        }
        catch {
            if (!disposed)
                state.error = "Alert settings could not be saved. Reload settings to verify the saved configuration.";
        }
        finally {
            state.busy = false;
            paint();
            if (!disposed)
                element.querySelector<HTMLElement>("#alerts-save")?.focus();
        }
    }
    async function permission() {
        if (disposed || state.busy || state.loading || !bridge.requestAlertPermission)
            return;
        state.busy = true;
        state.error = null;
        state.message = null;
        ++revision;
        paint();
        try {
            const result = await bridge.requestAlertPermission();
            if (disposed)
                return;
            state.view = result;
            if (!result.error)
                state.message = "Notification permission checked. No test notification was sent.";
        }
        catch {
            if (!disposed)
                state.error = "Notification permission could not be requested. No delivery is confirmed.";
        }
        finally {
            state.busy = false;
            paint();
            if (!disposed)
                element.querySelector<HTMLElement>("#alerts-permission")?.focus();
        }
    }
    async function sendTest() {
        if (disposed || state.busy || state.loading || state.view?.permission !== "granted" || !bridge.sendTestNotification)
            return;
        state.busy = true;
        state.error = null;
        state.message = null;
        ++revision;
        paint();
        try {
            await bridge.sendTestNotification();
            if (disposed)
                return;
            state.message = state.preview ? "Synthetic test simulated. No Windows notification was sent." : "Notification request queued. Visible Windows delivery is unverified.";
        }
        catch {
            if (!disposed)
                state.error = "The test notification could not be sent. Visible delivery is unverified.";
        }
        finally {
            state.busy = false;
            paint();
            if (!disposed)
                element.querySelector<HTMLElement>("#alerts-test")?.focus();
        }
    }
    paint();
    async function refreshStatus() {
        if (disposed || state.busy || state.loading || !bridge.getAlertSettings)
            return;
        const current = ++revision;
        try {
            const view = await bridge.getAlertSettings();
            if (!disposed && current === revision) {
                state.view = view;if(!dirty)adopt(view);
                paint();
            }
        }
        catch {
            if (!disposed && current === revision) {
                state.error = "Alert status could not be refreshed. Reload alert settings to verify delivery.";
                paint();
            }
        }
    }
    return { element, start: load, refreshStatus, dispose() { disposed = true; ++revision; } };
}
