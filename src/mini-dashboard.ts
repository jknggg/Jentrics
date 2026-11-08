import type {AgentId, DashboardState} from "./types.ts";
import type {MonitorSettingsView} from "./monitor-types.ts";
import {renderQuotaCountdown} from "./countdown.ts";

export interface MiniOptions {
    provider: AgentId;
    pinned: boolean;
    busy: boolean;
    preview: boolean;
    transportError: boolean;
    error: string | null;
    monitor: MonitorSettingsView | null;
}
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] {
    const element = document.createElement(tag);
    element.textContent = text;
    if (className) element.className = className;
    return element;
}
function iconButton(id: string, label: string, path: string): HTMLButtonElement {
    const button = node("button", "", "mini-icon-button");
    button.id = id; button.type = "button"; button.title = label; button.setAttribute("aria-label", label);
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24"); svg.setAttribute("aria-hidden", "true");
    const shape = document.createElementNS(svg.namespaceURI, "path");
    shape.setAttribute("d", path); svg.append(shape); button.append(svg);
    return button;
}
function freshness(state: DashboardState, options: MiniOptions): string {
    if (options.transportError) return "Local monitor unavailable";
    if (state.collectionError) return "Last check failed · Usage unverified";
    if (state.refreshing) return "Checking usage…";
    const agent = state.agents.find(agent => agent.agentId === options.provider);
    const quota = agent?.quota;
    if (agent?.availability === "probeFailed" || agent?.diagnosticCode) return "Check failed · Usage unverified";
    if (agent?.authState === "notAuthenticated") return "Sign-in required";
    if (!quota) return "Usage not reported";
    if (quota.diagnosticCode) return "Usage check failed";
    if (state.lastRefreshMs === null) return "Saved snapshot · Refresh to verify";
    const age = Date.now() - quota.observedMs;
    if (age < 0) return "Clock changed · Usage unverified";
    if (age >= 900000) return "Stale snapshot · Refresh to verify";
    return age < 60000 ? "Updated just now" : `Updated ${Math.floor(age / 60000)}m ago`;
}
export function renderMiniDashboard(state: DashboardState, options: MiniOptions): HTMLElement {
    const view = node("section", "", "mini-dashboard");
    view.setAttribute("aria-label", "Mini dashboard");
    const header = node("header", "", "mini-header"); header.id = "mini-drag";
    const brand = node("div", "", "mini-brand"); const mark = node("span", "J", "mini-mark"); mark.setAttribute("aria-hidden", "true");
    brand.append(mark, node("h1", "Jentrics")); header.append(brand);
    const controls = node("div", "", "mini-window-controls");
    const pin = iconButton("mini-pin", options.pinned ? "Unpin window" : "Keep on top", "m16 3 5 5-4 1-3 5v4l-3-3-6 6m6-6-5-5h4l5-3z");
    pin.setAttribute("aria-pressed", String(options.pinned)); pin.disabled = options.busy || options.preview;
    if (options.preview) pin.title = "Pin is available in the desktop app";
    const expand = iconButton("mini-expand", "Expand dashboard", "M9 4H4v5m11-5h5v5M4 15v5h5m11-5v5h-5");
    expand.disabled = options.busy;
    const close = iconButton("mini-close", options.preview ? "Leave mini preview" : "Close to tray", "m6 6 12 12M18 6 6 18");
    close.disabled = options.busy; controls.append(pin, expand, close); header.append(controls); view.append(header);

    const content = node("div", "", "mini-content");
    const select = node("select", "", "mini-provider"); select.id = "mini-provider"; select.setAttribute("aria-label", "Subscription");
    for (const [id, name] of [["codex", "Codex"], ["claude", "Claude Code"]] as const) {
        const option = node("option", name); option.value = id; option.selected = options.provider === id; select.append(option);
    }
    content.append(select, node("p", "Plan not reported", "mini-plan"));
    const agent = state.agents.find(agent => agent.agentId === options.provider);
    const quota = agent?.quota;
    const windows = node("div", "", "mini-windows");
    if (!quota) {
        const providerName = options.provider === "codex" ? "Codex" : "Claude";
        const message = agent?.diagnosticCode === "probeTimeout" ? `${providerName} check timed out. Refresh to try again.`
            : agent?.availability === "probeFailed" || agent?.diagnosticCode ? `${providerName} check failed. Open the full dashboard for details.`
            : agent?.authState === "notAuthenticated" ? `Sign in to ${providerName}, then refresh to check usage.`
            : options.provider === "claude" ? "No captured usage yet. Set up Claude capture in the full dashboard."
            : agent?.availability === "notFound" ? "Codex is not installed. Open the full dashboard to check setup."
            : agent?.availability === "unsupported" ? "Usage is unavailable for this Codex version."
            : "Usage not checked. Refresh to check Codex.";
        windows.append(node("p", message, "mini-empty"));
    } else if (quota.diagnosticCode) {
        windows.append(node("p", "Usage limits could not be read. Refresh to try again.", "mini-empty"));
    } else if (!quota.windows.length) {
        windows.append(node("p", "Usage limits not reported for this subscription.", "mini-empty"));
    } else {
        for (const window of quota.windows) {
            const section = node("article", "", "mini-quota");
            const minutes = window.windowMinutes;
            const label = minutes === null ? "Usage window" : minutes % 1440 === 0 ? `${minutes / 1440}-day window`
                : minutes % 60 === 0 ? `${minutes / 60}-hour window` : `${minutes}-minute window`;
            const row = node("div", "", "mini-quota-heading"); row.append(node("h2", label));
            row.append(node("span", window.usedPercent === null ? "Usage not reported" : `${new Intl.NumberFormat(undefined, {maximumFractionDigits: 2}).format(window.usedPercent)}% used`));
            section.append(row);
            // Keep provider bucket identities available when durations overlap or are unknown.
            if (minutes === null || quota.windows.filter(item => item.windowMinutes === minutes).length > 1) section.append(node("p", window.bucketId, "mini-bucket"));
            section.title = window.bucketId;
            if (window.usedPercent !== null) {
                const meter = node("meter"); meter.min = 0; meter.max = 100; meter.value = window.usedPercent;
                meter.setAttribute("aria-label", `${window.bucketId}, ${label}, usage`); section.append(meter);
            }
            const reset = node("p", "", "mini-countdown");
            reset.append(renderQuotaCountdown(window.resetsAtMs, window.observedMs, quota.source === "claudeStatusLine")); section.append(reset);
            if (window.resetsAtMs !== null) {
                const date = new Date(window.resetsAtMs);
                const time = node("time", date.toLocaleString(undefined, {weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit", timeZoneName: "short"}));
                time.dateTime = date.toISOString(); time.title = date.toLocaleString(); section.append(time);
            }
            windows.append(section);
        }
    }
    content.append(windows); view.append(content);
    const footer = node("footer", "", "mini-footer"); const status = node("div", "", "mini-status");
    const fresh = node("p", freshness(state, options), "mini-freshness"); fresh.setAttribute("role", "status"); status.append(fresh);
    const monitor = options.monitor;
    const next = monitor?.settings?.paused ? "Automatic checks paused"
        : monitor?.runtimeReason === "noQualifiedProviders" ? "No automatic checks available"
        : !monitor || monitor.error || !monitor.settings ? "Next check unavailable"
        : monitor.nextCheckMs === null ? "Next check not scheduled"
        : monitor.nextCheckMs <= Date.now() ? "Next check due"
        : `Next check ${new Date(monitor.nextCheckMs).toLocaleTimeString(undefined, {hour: "numeric", minute: "2-digit"})}`;
    status.append(node("p", next, "mini-next-check"));
    if (quota?.source === "claudeStatusLine") status.append(node("p", "From saved Claude capture", "mini-next-check"));
    const refresh = iconButton("refresh", state.refreshing ? "Checking usage" : "Refresh now", "M20 7v5h-5M4 17v-5h5M6 7a7 7 0 0 1 11-2l3 3M4 16l3 3a7 7 0 0 0 11-2");
    refresh.disabled = state.refreshing; footer.append(status, refresh); view.append(footer);
    for (const message of [options.error, state.storageError ? "Local history unavailable" : null, options.preview ? "Browser preview · synthetic data" : null]) {
        if (message) {const notice = node("p", message, "mini-notice"); notice.setAttribute("role", "status"); view.append(notice);}
    }
    return view;
}
