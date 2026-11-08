import type { AlertSettings, AlertSettingsView, WindowsNotificationState } from "./alerts-types.ts";
export interface AlertsUiState {
    preview: boolean;
    available: boolean;
    canSave: boolean;
    canRequestPermission: boolean;
    canSendTest: boolean;
    view: AlertSettingsView | null;
    draft: AlertSettings | null;
    quietEnabled: boolean;
    quietStart: string;
    quietEnd: string;
    loading: boolean;
    busy: boolean;
    error: string | null;
    message: string | null;
}
const permissions = { granted: "App notification API reports granted. Windows notification settings and Do Not Disturb can still suppress delivery. This does not verify Windows toast delivery.", denied: "App notification API permission denied. Alerts cannot appear while API permission is denied.", prompt: "App notification API permission has not been granted. Request permission separately before expecting alerts.", unavailable: "Notification API permission is unavailable in this environment. Windows delivery is unverified." };
const windowsStates: Record<WindowsNotificationState, string> = {
    accepting: "Windows reports it can accept notifications. This does not guarantee a visible toast.",
    busy: "Windows reports you are busy. Full-screen or presentation activity may suppress notification popups.",
    notPresent: "Windows reports that the user is not present. Popup visibility is unverified.",
    fullScreen: "Windows reports a full-screen application. Notification popups may be suppressed.",
    presentation: "Windows reports presentation mode. Notification popups may be suppressed.",
    quietTime: "Windows reports quiet time. Notification popups may be suppressed.",
    storeApp: "Windows reports a Windows Store app is running. Visible notification delivery is unverified.",
    unavailable: "Windows interruption status is unavailable. A queued request does not confirm a visible popup."
};
const errors = { settingsUnavailable: "Alert settings are unavailable. No saved configuration is confirmed.", deliveryUnavailable: "Delivery is unavailable. The last notification could not be delivered; saved alert settings do not prove Windows toast delivery.", permissionUnavailable: "Notification API permission could not be checked. Windows delivery is unverified." };
export const alertChecks = [['enabled', 'Enable quota alerts'], ['threshold80', '80% usage threshold'], ['threshold95', '95% usage threshold'], ['nearReset', 'Expected reset within 10 minutes'], ['expectedReset', 'Expected reset time passed · verify usage'], ['restored', 'Fresh reported room after a reset'], ['errors', 'Monitoring error advisories']] as const;
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] {
    const result = document.createElement(tag);
    result.textContent = text;
    if (className)
        result.className = className;
    return result;
}
function checkbox(id: string, text: string, checked: boolean, disabled: boolean): HTMLElement { const label = node("label", "", "alerts-check"); const input = node("input"); input.type = "checkbox"; input.id = id; input.checked = checked; input.disabled = disabled; label.append(input, document.createTextNode(text)); return label; }
export function renderAlerts(state: AlertsUiState): HTMLElement {
    const section = node("section", "", "alerts-section");
    section.setAttribute("aria-labelledby", "alerts-title");
    const title = node("h2", "Quota alerts");
    title.id = "alerts-title";
    section.append(title, node("p", "Optional Windows notifications from fresh reported quota observations. Expected reset times do not confirm restored capacity. Alerts never send model requests.", "quota-meta"));
    if (state.preview)
        section.append(node("p", "Synthetic preview only. Settings stay in preview memory. No Windows notification or permission request is sent.", "quota-meta"));
    if (!state.available) {
        section.append(node("p", "Alerts are unavailable in this connection.", "quota-meta"));
        return section;
    }
    const status = node("p", state.loading ? "Loading alert settings…" : state.view?.settings.enabled ? "Saved setting: alerts on" : "Saved setting: alerts off", "quota-meta");
    if (!state.view && !state.loading)
        status.textContent = "Saved alert setting unknown";
    status.setAttribute("role", "status");
    status.setAttribute("aria-live", "polite");
    section.append(status);
    if (state.view) {
        section.append(node("p", permissions[state.view.permission], "quota-meta"));
        section.append(node("p", windowsStates[state.view.windowsState ?? "unavailable"], "quota-meta"), node("p", "Jentrics does not change Windows notification settings.", "quota-meta"));
        if (state.view.error)
            section.append(node("p", errors[state.view.error], "error-notice"));
    }
    if (state.draft) {
        const checks = node("div", "", "alerts-options");
        for (const [key, label] of alertChecks)
            checks.append(checkbox(`alerts-${key}`, label, state.draft[key], state.busy || state.loading));
        checks.append(checkbox("alerts-quiet", "Quiet hours using this device's local time", state.quietEnabled, state.busy || state.loading));
        section.append(checks);
        const times = node("div", "", "history-controls");
        for (const [id, label, value] of [["alerts-quiet-start", "Quiet hours start", state.quietStart], ["alerts-quiet-end", "Quiet hours end", state.quietEnd]]) {
            const wrapper = node("label", label, "history-field");
            const input = node("input");
            input.type = "time";
            input.step = "60";
            input.id = id!;
            input.value = value!;
            input.disabled = !state.quietEnabled || state.busy || state.loading;
            wrapper.append(input);
            times.append(wrapper);
        }
        section.append(times, node("p", "Quiet hours can cross midnight. Automatic notifications are paused during quiet hours. Changes take effect only after Save alerts.", "quota-meta"));
    }
    const actions = node("div", "", "history-controls");
    for (const [id, text, disabled] of [["alerts-save", state.busy ? "Working…" : "Save alerts", !state.draft || !state.canSave || state.busy || state.loading], ["alerts-permission", "Request notification permission", !state.canRequestPermission || state.busy || state.loading || state.view?.permission === "granted"], ["alerts-test", "Send test notification", !state.canSendTest || state.busy || state.loading || state.view?.permission !== "granted"], ["alerts-reload", "Reload alert settings", state.busy || state.loading]] as const) {
        const button = node("button", text, "wake-button");
        button.id = id;
        button.type = "button";
        button.disabled = disabled;
        actions.append(button);
    }
    section.append(actions);
    section.append(node("p", "Manual test notifications can appear immediately even when automatic alerts are off or quiet hours are active. Sending a test does not change automatic alert settings.", "quota-meta"));
    if (!state.canSendTest)
        section.append(node("p", "Sending a manual test is unavailable in this connection.", "quota-meta"));
    if (!state.canSave)
        section.append(node("p", "Changing alert settings is unavailable in this connection.", "quota-meta"));
    if (!state.canRequestPermission)
        section.append(node("p", "Requesting notification permission is unavailable in this connection.", "quota-meta"));
    for (const [text, className] of [[state.error, "error-notice"], [state.message, "quota-meta"]])
        if (text) {
            const message = node("p", text, className!);
            message.setAttribute("role", "status");
            message.setAttribute("aria-live", "polite");
            section.append(message);
        }
    return section;
}
