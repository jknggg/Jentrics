import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { createMonitorController } from "../src/monitor-controller.ts";
import type { MonitorBridge, MonitorSettings, MonitorSettingsView } from "../src/monitor-types.ts";
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
function view(): MonitorSettingsView { return { settings: { paused: false, intervalMinutes: 5 }, error: null, nextCheckMs: null, effectiveIntervalMinutes: 5, lastClockChangeMs: null }; }
async function mount(overrides: Partial<MonitorBridge> = {}) { const { document } = parseHTML("<html><body><main id='root'></main></body></html>"); Object.defineProperty(globalThis, "document", { value: document, configurable: true }); const saves: MonitorSettings[] = []; const bridge: MonitorBridge = { getMonitorSettings: async () => view(), setMonitorSettings: async (settings) => { saves.push(settings); return { ...view(), settings }; }, ...overrides }; const root = document.querySelector("#root") as unknown as HTMLElement; const controller = createMonitorController(bridge); root.append(controller.element); await controller.start(); return { root, controller, saves }; }
function select(root: HTMLElement, id: string, value: string) { const input = root.querySelector<HTMLSelectElement>(`#${id}`)!; Array.from(input.options).forEach(option => { option.selected = false; }); input.querySelector<HTMLOptionElement>(`option[value="${value}"]`)!.selected = true; input.dispatchEvent(new input.ownerDocument.defaultView!.Event("change")); }
test("monitor loads saved five-minute setting without any implicit write", async () => { const app = await mount(); try {
    assert.equal(app.root.querySelector<HTMLSelectElement>("#monitor-interval")!.value, "5");
    assert.equal(app.saves.length, 0);
    assert.match(app.root.textContent ?? "", /Automatic monitoring enabled/);
    select(app.root, "monitor-mode", "paused");
    select(app.root, "monitor-interval", "15");
    app.root.querySelector<HTMLButtonElement>("#monitor-save")!.click();
    await flush();
    assert.deepEqual(app.saves, [{ paused: true, intervalMinutes: 15 }]);
    assert.match(app.root.textContent ?? "", /Automatic monitoring paused/);
    assert.match(app.root.textContent ?? "", /Refresh now remains available/);
}
finally {
    app.controller.dispose();
} });
test("unknown settings are explicit and require deliberate recovery choices", async () => { const app = await mount({ getMonitorSettings: async () => ({ ...view(), settings: null, error: "settingsUnavailable", effectiveIntervalMinutes: null }) }); try {
    assert.equal(app.root.querySelector<HTMLSelectElement>("#monitor-mode")!.value, "");
    assert.equal(app.root.querySelector<HTMLSelectElement>("#monitor-interval")!.value, "");
    app.root.querySelector<HTMLButtonElement>("#monitor-save")!.click();
    assert.equal(app.saves.length, 0);
    assert.match(app.root.textContent ?? "", /Choose.*monitoring.*interval/i);
    select(app.root, "monitor-mode", "running");
    select(app.root, "monitor-interval", "10");
    app.root.querySelector<HTMLButtonElement>("#monitor-save")!.click();
    await flush();
    assert.deepEqual(app.saves, [{ paused: false, intervalMinutes: 10 }]);
}
finally {
    app.controller.dispose();
} });
test("bounded backoff and detected clock changes are shown as observations", async () => { const app = await mount({ getMonitorSettings: async () => ({ ...view(), nextCheckMs: Date.now() + 1200000, effectiveIntervalMinutes: 20, lastClockChangeMs: Date.now() - 1000 }) }); try {
    assert.match(app.root.textContent ?? "", /20 minutes/);
    assert.match(app.root.textContent ?? "", /collection or quota failures/);
    assert.match(app.root.textContent ?? "", /Clock or scheduling change detected/);
    assert.match(app.root.textContent ?? "", /Next scheduled check/);
}
finally {
    app.controller.dispose();
} });
test("missing methods and failed settings reads do not invent running defaults", async () => { const app = await mount({ getMonitorSettings: undefined }); try {
    assert.match(app.root.textContent ?? "", /unavailable in this connection/);
}
finally {
    app.controller.dispose();
} const failed = await mount({ getMonitorSettings: async () => { throw new Error("SECRET"); } }); try {
    assert.match(failed.root.textContent ?? "", /could not be loaded/);
    assert.doesNotMatch(failed.root.textContent ?? "", /SECRET|Automatic monitoring enabled/);
}
finally {
    failed.controller.dispose();
} });
test("failed saves stay visible and do not claim monitoring changed", async () => { const app = await mount({ setMonitorSettings: async () => { throw new Error("SECRET"); } }); try {
    select(app.root, "monitor-mode", "paused");
    app.root.querySelector<HTMLButtonElement>("#monitor-save")!.click();
    await flush();
    assert.match(app.root.textContent ?? "", /could not be saved/);
    assert.doesNotMatch(app.root.textContent ?? "", /SECRET|settings saved/i);
}
finally {
    app.controller.dispose();
} });
test("disposal ignores late settings results and a pending save is single-flight", async () => { let resolve!: (view: MonitorSettingsView) => void; let calls = 0; const pending = new Promise<MonitorSettingsView>(yes => { resolve = yes; }); const app = await mount({ setMonitorSettings: async () => { calls++; return pending; } }); app.root.querySelector<HTMLButtonElement>("#monitor-save")!.click(); app.root.querySelector<HTMLButtonElement>("#monitor-save")!.click(); assert.equal(calls, 1); app.controller.dispose(); const text = app.root.textContent; resolve(view()); await flush(); assert.equal(app.root.textContent, text); });
