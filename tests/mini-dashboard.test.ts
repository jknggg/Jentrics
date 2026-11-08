import {test} from "node:test";
import assert from "node:assert/strict";
import {parseHTML} from "linkedom";
import {renderMiniDashboard} from "../src/mini-dashboard.ts";
import {mountDashboard} from "../src/controller.ts";
import type {Bridge, DashboardState} from "../src/types.ts";
import {previewBridge} from "../src/preview.ts";

function dom() {
    const {document} = parseHTML("<html><body><main id='root'></main></body></html>");
    Object.defineProperty(globalThis, "document", {value: document, configurable: true});
    return document.querySelector("#root") as unknown as HTMLElement;
}
function sample(): DashboardState {
    return {schemaVersion: 1, refreshing: false, lastRefreshMs: Date.now(), storageError: null, collectionError: null,
        agents: [{agentId: "codex", version: "fixture", availability: "detected", authState: "ready", diagnosticCode: null,
            observedMs: Date.now(), capabilities: {discovery: true, authReadiness: true, quotas: true, usage: false, sessionStart: false},
            quota: {source: "codexAppServer", observedMs: Date.now(), diagnosticCode: null,
                windows: [{bucketId: "codex:primary", windowMinutes: 300, usedPercent: 25, observedMs: Date.now(), resetsAtMs: Date.now() + 7200000}]}}]};
}
const options = {provider: "codex" as const, pinned: false, busy: false, preview: false, transportError: false, error: null, monitor: null};
const settle = () => new Promise(resolve => setTimeout(resolve, 0));

test("mini view shows one provider, truthful plan availability, quota and absolute reset", () => {
    dom(); const state = sample(); const view = renderMiniDashboard(state, options);
    assert.match(view.textContent!, /Plan not reported/);
    assert.match(view.textContent!, /25% used/);
    assert.match(view.textContent!, /Resets in 2h/);
    assert.equal(view.querySelectorAll("meter").length, 1);
    assert.equal(view.querySelector("time")?.getAttribute("datetime"), new Date(state.agents[0]!.quota!.windows[0]!.resetsAtMs!).toISOString());
    assert.equal(view.querySelector("#mini-provider")?.getAttribute("aria-label"), "Subscription");
});
test("unknown usage, stale snapshots and elapsed resets never imply replenishment", () => {
    dom(); const state = sample(); const quota = state.agents[0]!.quota!;
    quota.observedMs -= 960000; quota.windows[0]!.usedPercent = null; quota.windows[0]!.resetsAtMs = Date.now() - 1;
    const view = renderMiniDashboard(state, options);
    assert.match(view.textContent!, /Usage not reported/); assert.match(view.textContent!, /Stale/);
    assert.match(view.textContent!, /Waiting for fresh quota/); assert.equal(view.querySelector("meter"), null);
    assert.doesNotMatch(view.textContent!, /0% used|100% remaining/);
    assert.match(renderMiniDashboard(state, {...options, provider: "claude"}).textContent!, /No captured usage/);
    quota.diagnosticCode = "quotaUnavailable";
    assert.equal(renderMiniDashboard(state, options).querySelector("meter"), null);
    assert.match(renderMiniDashboard(state, {...options, transportError: true}).textContent!, /Local monitor unavailable/);
});
test("mini mode shares subscription and refresh, retains provider selection and expands", async () => {
    const root = dom(); let refreshes = 0; let subscriptions = 0; const transitions: boolean[] = [];
    const bridge: Bridge = {preview: false, getState: async () => sample(), refresh: async () => {refreshes++; return sample();},
        subscribe: async () => {subscriptions++; return () => {};}};
    const controller = await mountDashboard(root, bridge, {setCompact: async value => {transitions.push(value);}, setPinned: async () => {}, close: async () => {}, drag: async () => {}});
    try {
        root.querySelector<HTMLButtonElement>("#mini-open")!.click(); await settle();
        assert.ok(root.querySelector(".mini-dashboard")); assert.equal(root.querySelector(".agents-table"), null);
        const select = root.querySelector<HTMLSelectElement>("#mini-provider")!;
        select.querySelector<HTMLOptionElement>('option[value="codex"]')!.selected = false;
        select.querySelector<HTMLOptionElement>('option[value="claude"]')!.selected = true;
        select.dispatchEvent(new document.defaultView!.Event("change"));
        root.querySelector<HTMLButtonElement>("#refresh")!.click(); await settle();
        assert.equal(root.querySelector<HTMLSelectElement>("#mini-provider")!.value, "claude");
        assert.equal(refreshes, 1); assert.equal(subscriptions, 1);
        root.querySelector<HTMLButtonElement>("#mini-expand")!.click(); await settle();
        assert.ok(root.querySelector(".agents-table")); assert.deepEqual(transitions, [true, false]);
    } finally {controller.dispose();}
});
test("failed native shrink stays full and reports the error", async () => {
    const root = dom(); const bridge: Bridge = {preview: false, getState: async () => sample(), refresh: async () => sample(), subscribe: async () => () => {}};
    const controller = await mountDashboard(root, bridge, {setCompact: async () => {throw Error("Window refused resize");}, setPinned: async () => {}, close: async () => {}, drag: async () => {}});
    try {
        root.querySelector<HTMLButtonElement>("#mini-open")!.click(); await settle();
        assert.ok(root.querySelector(".agents-table")); assert.match(root.textContent!, /Window refused resize/);
    } finally {controller.dispose();}
});
test("mini view keeps failed checks and sign-in requirements visible", () => {
    dom(); const state = sample(); const agent = state.agents[0]!; agent.quota = null;
    agent.availability = "probeFailed"; agent.diagnosticCode = "probeTimeout";
    assert.match(renderMiniDashboard(state, options).textContent!, /check timed out/i);
    assert.match(renderMiniDashboard(state, options).textContent!, /Check failed/);
    agent.availability = "detected"; agent.diagnosticCode = null; agent.authState = "notAuthenticated";
    assert.match(renderMiniDashboard(state, options).textContent!, /Sign in to Codex/);
    assert.match(renderMiniDashboard(state, options).textContent!, /Sign-in required/);
});
test("pushed quota and monitoring updates reach mini mode without losing full-view drafts", async () => {
    const root = dom(); let update: (state: DashboardState) => void = () => {};
    const bridge = previewBridge(); bridge.subscribe = async listener => {update = listener; return () => {};};
    const controller = await mountDashboard(root, bridge);
    try {
        const input = root.querySelector<HTMLInputElement>("#pricing-count-output")!;
        input.value = "123"; input.dispatchEvent(new document.defaultView!.Event("input"));
        root.querySelector<HTMLButtonElement>("#mini-open")!.click(); await settle();
        const fresh = sample(); fresh.agents[0]!.quota!.windows[0]!.usedPercent = 68;
        await bridge.setMonitorSettings!({paused: true, intervalMinutes: 5});
        update(fresh); await settle();
        assert.match(root.textContent!, /68% used/); assert.match(root.textContent!, /Automatic checks paused/);
        root.querySelector<HTMLButtonElement>("#mini-expand")!.click(); await settle();
        assert.equal(root.querySelector<HTMLInputElement>("#pricing-count-output")!.value, "123");
        assert.match(root.querySelector(".quota-sections")!.textContent!, /68% used/);
    } finally {controller.dispose();}
});
test("tray Settings awaits native expansion before focusing configuration", async () => {
    const root = dom(); let notify: () => void = () => {}; let pending = false; let release: () => void = () => {};
    let focused = false;
    const originalFocus = document.defaultView!.HTMLElement.prototype.focus;
    document.defaultView!.HTMLElement.prototype.focus = function () {if (this.id === "configuration-title") focused = true;};
    const bridge = previewBridge();
    bridge.subscribeSettings = async listener => {notify = listener; return () => {};};
    bridge.takeSettingsRequest = async () => {const requested = pending; pending = false; return requested;};
    const controller = await mountDashboard(root, bridge, {setCompact: async value => {if (!value) await new Promise<void>(resolve => {release = resolve;});}, setPinned: async () => {}, close: async () => {}, drag: async () => {}});
    try {
        root.querySelector<HTMLButtonElement>("#mini-open")!.click(); await settle();
        pending = true; notify(); await settle();
        assert.ok(root.querySelector(".mini-dashboard")); assert.equal(focused, false);
        release(); await settle();
        assert.ok(root.querySelector("#configuration-title")); assert.equal(focused, true);
    } finally {controller.dispose(); document.defaultView!.HTMLElement.prototype.focus = originalFocus;}
});
test("failed Settings expansion keeps mini mode and shows the native error", async () => {
    const root = dom(); let notify: () => void = () => {}; let pending = false;
    const bridge = previewBridge(); bridge.subscribeSettings = async listener => {notify = listener; return () => {};};
    bridge.takeSettingsRequest = async () => {const requested = pending; pending = false; return requested;};
    const controller = await mountDashboard(root, bridge, {setCompact: async value => {if (!value) throw Error("Expansion failed");}, setPinned: async () => {}, close: async () => {}, drag: async () => {}});
    try {
        root.querySelector<HTMLButtonElement>("#mini-open")!.click(); await settle();
        pending = true; notify(); await settle();
        assert.ok(root.querySelector(".mini-dashboard")); assert.match(root.textContent!, /Expansion failed/);
    } finally {controller.dispose();}
});
