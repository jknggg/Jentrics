import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { createAlertsController } from "../src/alerts-controller.ts";
import type { AlertsBridge, AlertSettings, AlertSettingsView } from "../src/alerts-types.ts";
import { previewBridge } from "../src/preview.ts";
import { mountDashboard } from "../src/controller.ts";
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
function view(): AlertSettingsView { return { settings: { enabled: false, threshold80: true, threshold95: true, nearReset: true, expectedReset: true, restored: true, errors: true, quietHours: null }, permission: "prompt", error: null }; }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes; }); return { promise, resolve }; }
async function mount(overrides: Partial<AlertsBridge> = {}) { const { document } = parseHTML("<html><body><main id='root'></main></body></html>"); Object.defineProperty(globalThis, "document", { value: document, configurable: true }); const saves: AlertSettings[] = []; let permissions = 0; const bridge: AlertsBridge = { getAlertSettings: async () => view(), setAlertSettings: async (settings) => { saves.push(settings); return { ...view(), settings }; }, requestAlertPermission: async () => { permissions++; return { ...view(), permission: "granted" }; }, ...overrides }; const root = document.querySelector("#root") as unknown as HTMLElement; const controller = createAlertsController(bridge); root.append(controller.element); await controller.start(); return { root, controller, saves, permissions: () => permissions }; }
function check(root: HTMLElement, id: string, checked: boolean) { const input = root.querySelector<HTMLInputElement>(`#${id}`)!; input.checked = checked; input.dispatchEvent(new input.ownerDocument.defaultView!.Event("change")); }
function time(root: HTMLElement, id: string, value: string) { const input = root.querySelector<HTMLInputElement>(`#${id}`)!; input.value = value; input.dispatchEvent(new input.ownerDocument.defaultView!.Event("input")); }
test("alerts load off from backend and require explicit save without requesting permission", async () => {
    const app = await mount();
    try {
        assert.equal(app.root.querySelector<HTMLInputElement>("#alerts-enabled")!.checked, false);
        assert.equal(app.saves.length, 0);
        assert.equal(app.permissions(), 0);
        assert.match(app.root.textContent ?? "", /80%/);
        assert.match(app.root.textContent ?? "", /95%/);
        assert.match(app.root.textContent ?? "", /10 minutes/);
        check(app.root, "alerts-enabled", true);
        app.root.querySelector<HTMLButtonElement>("#alerts-save")!.click();
        await flush();
        assert.equal(app.saves[0]!.enabled, true);
        assert.equal(app.permissions(), 0);
        assert.match(app.root.textContent ?? "", /settings saved/i);
    }
    finally {
        app.controller.dispose();
    }
});
test("quiet hours validate explicit local times and save midnight wrapping values", async () => {
    const app = await mount();
    try {
        check(app.root, "alerts-quiet", true);
        app.root.querySelector<HTMLButtonElement>("#alerts-save")!.click();
        assert.equal(app.saves.length, 0);
        assert.match(app.root.textContent ?? "", /Enter.*start.*end/);
        time(app.root, "alerts-quiet-start", "22:00");
        time(app.root, "alerts-quiet-end", "07:00");
        app.root.querySelector<HTMLButtonElement>("#alerts-save")!.click();
        await flush();
        assert.deepEqual(app.saves[0]!.quietHours, { startMinutes: 1320, endMinutes: 420 });
    }
    finally {
        app.controller.dispose();
    }
});
test("equal quiet times fail visibly and disabling quiet hours stores null", async () => {
    const app = await mount();
    try {
        check(app.root, "alerts-quiet", true);
        time(app.root, "alerts-quiet-start", "07:00");
        time(app.root, "alerts-quiet-end", "07:00");
        app.root.querySelector<HTMLButtonElement>("#alerts-save")!.click();
        assert.equal(app.saves.length, 0);
        assert.match(app.root.textContent ?? "", /must differ/);
        check(app.root, "alerts-quiet", false);
        app.root.querySelector<HTMLButtonElement>("#alerts-save")!.click();
        await flush();
        assert.equal(app.saves[0]!.quietHours, null);
    }
    finally {
        app.controller.dispose();
    }
});
test("permission is requested only by its separate explicit action", async () => {
    const app = await mount();
    try {
        app.root.querySelector<HTMLButtonElement>("#alerts-permission")!.click();
        await flush();
        assert.equal(app.permissions(), 1);
        assert.equal(app.saves.length, 0);
        assert.match(app.root.textContent ?? "", /granted/i);
    }
    finally {
        app.controller.dispose();
    }
});
test("denied or unavailable delivery never promises Windows toast success", async () => {
    for (const state of [{ ...view(), permission: "denied" as const }, { ...view(), permission: "unavailable" as const, error: "deliveryUnavailable" as const }]) {
        const app = await mount({ getAlertSettings: async () => state });
        try {
            assert.match(app.root.textContent ?? "", state.permission === "denied" ? /denied/i : /Delivery.*unavailable/i);
            assert.doesNotMatch(app.root.textContent ?? "", /notification delivered/i);
        }
        finally {
            app.controller.dispose();
        }
    }
});
test("settings failures expose no fake defaults or raw error data", async () => {
    const app = await mount({ getAlertSettings: async () => { throw new Error("SECRET"); } });
    try {
        assert.match(app.root.textContent ?? "", /settings could not be loaded/i);
        assert.equal(app.root.querySelector("#alerts-enabled"), null);
        assert.doesNotMatch(app.root.textContent ?? "", /SECRET/);
    }
    finally {
        app.controller.dispose();
    }
    const unavailable = await mount({ getAlertSettings: undefined });
    try {
        assert.match(unavailable.root.textContent ?? "", /unavailable in this connection/);
    }
    finally {
        unavailable.controller.dispose();
    }
});
test("save failures stay visible, pending save is single-flight, disposal ignores late results", async () => {
    const pending = deferred<AlertSettingsView>();
    let calls = 0;
    const app = await mount({ setAlertSettings: async () => { calls++; return pending.promise; } });
    app.root.querySelector<HTMLButtonElement>("#alerts-save")!.click();
    app.root.querySelector<HTMLButtonElement>("#alerts-save")!.click();
    assert.equal(calls, 1);
    app.controller.dispose();
    const text = app.root.textContent;
    pending.resolve(view());
    await flush();
    assert.equal(app.root.textContent, text);
    const failed = await mount({ setAlertSettings: async () => { throw new Error("SECRET"); } });
    try {
        failed.root.querySelector<HTMLButtonElement>("#alerts-save")!.click();
        await flush();
        assert.match(failed.root.textContent ?? "", /settings could not be saved/i);
        assert.doesNotMatch(failed.root.textContent ?? "", /SECRET/);
    }
    finally {
        failed.controller.dispose();
    }
});
test("permission API status never certifies Windows delivery or discards unsaved choices", async () => {
    const app = await mount();
    try {
        check(app.root, "alerts-enabled", true);
        app.root.querySelector<HTMLButtonElement>("#alerts-permission")!.click();
        await flush();
        assert.equal(app.root.querySelector<HTMLInputElement>("#alerts-enabled")!.checked, true);
        assert.match(app.root.textContent ?? "", /App notification API reports granted/);
        assert.match(app.root.textContent ?? "", /Do Not Disturb/);
        assert.match(app.root.textContent ?? "", /does not verify Windows toast delivery/);
    }
    finally {
        app.controller.dispose();
    }
});
test("mounted preview alerts save synthetic memory and never grant real Windows permission", async () => {
    const { document } = parseHTML("<html><body><main id='root'></main></body></html>");
    Object.defineProperty(globalThis, "document", { value: document, configurable: true });
    const root = document.querySelector("#root") as unknown as HTMLElement;
    const bridge = previewBridge();
    const controller = await mountDashboard(root, bridge);
    try {
        assert.equal((await bridge.getAlertSettings!()).settings.enabled, false);
        check(root, "alerts-enabled", true);
        root.querySelector<HTMLButtonElement>("#alerts-save")!.click();
        await flush();
        assert.equal((await bridge.getAlertSettings!()).settings.enabled, true);
        root.querySelector<HTMLButtonElement>("#alerts-permission")!.click();
        await flush();
        assert.equal((await bridge.getAlertSettings!()).permission, "unavailable");
        assert.match(root.querySelector(".alerts-section")!.textContent ?? "", /Synthetic preview only/);
    }
    finally {
        controller.dispose();
    }
});
test("manual notification test is explicit and never sent by load, save or permission", async () => {
    let tests = 0;
    const app = await mount({ sendTestNotification: async () => { tests++; } });
    try {
        assert.equal(tests, 0);
        assert.equal(app.root.querySelector<HTMLButtonElement>('#alerts-test')!.disabled, true);
        app.root.querySelector<HTMLButtonElement>('#alerts-save')!.click();
        await flush();
        assert.equal(tests, 0);
        app.root.querySelector<HTMLButtonElement>('#alerts-permission')!.click();
        await flush();
        assert.equal(tests, 0);
        const writes = app.saves.length;
        app.root.querySelector<HTMLButtonElement>('#alerts-test')!.click();
        await flush();
        assert.equal(tests, 1);
        assert.equal(app.saves.length, writes);
        assert.equal(app.saves.at(-1)!.enabled, false);
        assert.match(app.root.textContent ?? '', /Notification request queued\. Visible Windows delivery is unverified\./);
        assert.match(app.root.textContent ?? '', /Manual.*immediately.*automatic alerts.*off.*quiet hours/i);
    }
    finally {
        app.controller.dispose();
    }
});
test("manual notification test requires both granted permission and a connected method", async () => {
    for (const permission of ['prompt', 'denied', 'unavailable'] as const) {
        let tests = 0;
        const app = await mount({ getAlertSettings: async () => ({ ...view(), permission }), sendTestNotification: async () => { tests++; } });
        try {
            assert.equal(app.root.querySelector<HTMLButtonElement>('#alerts-test')!.disabled, true);
            app.root.querySelector<HTMLButtonElement>('#alerts-test')!.click();
            await flush();
            assert.equal(tests, 0);
        }
        finally {
            app.controller.dispose();
        }
    }
    const absent = await mount({ getAlertSettings: async () => ({ ...view(), permission: 'granted' }), sendTestNotification: undefined });
    try {
        assert.equal(absent.root.querySelector<HTMLButtonElement>('#alerts-test')!.disabled, true);
        assert.match(absent.root.textContent ?? '', /manual test.*unavailable in this connection/i);
    }
    finally {
        absent.controller.dispose();
    }
});
test("manual notification test is single-flight and disposal ignores a late acceptance", async () => {
    const pending = deferred<void>();
    let calls = 0;
    const app = await mount({ getAlertSettings: async () => ({ ...view(), permission: 'granted' }), sendTestNotification: async () => { calls++; return pending.promise; } });
    app.root.querySelector<HTMLButtonElement>('#alerts-test')!.click();
    app.root.querySelector<HTMLButtonElement>('#alerts-test')!.click();
    assert.equal(calls, 1);
    assert.equal(app.root.querySelector<HTMLButtonElement>('#alerts-test')!.disabled, true);
    app.controller.dispose();
    const text = app.root.textContent;
    pending.resolve(undefined);
    await flush();
    assert.equal(app.root.textContent, text);
});
test("failed notification test stays explicit and preview only simulates acceptance", async () => {
    const failed = await mount({ getAlertSettings: async () => ({ ...view(), permission: 'granted' }), sendTestNotification: async () => { throw new Error('SECRET'); } });
    try {
        failed.root.querySelector<HTMLButtonElement>('#alerts-test')!.click();
        await flush();
        assert.match(failed.root.textContent ?? '', /test notification could not be sent/i);
        assert.doesNotMatch(failed.root.textContent ?? '', /SECRET|Notification request queued/);
    }
    finally {
        failed.controller.dispose();
    }
    const simulated = await mount({ preview: true, getAlertSettings: async () => ({ ...view(), permission: 'granted' }), sendTestNotification: async () => { } });
    try {
        simulated.root.querySelector<HTMLButtonElement>('#alerts-test')!.click();
        await flush();
        assert.match(simulated.root.textContent ?? '', /Synthetic.*simulated.*No Windows notification was sent/);
        assert.doesNotMatch(simulated.root.textContent ?? '', /Notification request queued/);
    }
    finally {
        simulated.controller.dispose();
    }
});
test("background alert status updates delivery errors while preserving unsaved choices", async () => {
    const app = await mount({ getAlertSettings: async () => ({ ...view(), error: 'deliveryUnavailable' }) });
    try {
        check(app.root, 'alerts-enabled', true);
        check(app.root, 'alerts-quiet', true);
        time(app.root, 'alerts-quiet-start', '22:00');
        time(app.root, 'alerts-quiet-end', '07:00');
        await app.controller.refreshStatus();
        assert.equal(app.root.querySelector<HTMLInputElement>('#alerts-enabled')!.checked, true);
        assert.equal(app.root.querySelector<HTMLInputElement>('#alerts-quiet-start')!.value, '22:00');
        assert.equal(app.root.querySelector<HTMLInputElement>('#alerts-quiet-end')!.value, '07:00');
        assert.match(app.root.textContent ?? '', /Delivery is unavailable/);
        assert.equal(app.saves.length, 0);
    }
    finally {
        app.controller.dispose();
    }
});
test("Windows busy state explains suppressed popups without changing settings or disabling an explicit test", async () => {
    let tests = 0;
    const app = await mount({ getAlertSettings: async () => ({ ...view(), permission: 'granted', windowsState: 'busy' }), sendTestNotification: async () => { tests++; } });
    try {
        assert.match(app.root.textContent ?? '', /Windows reports.*busy/);
        assert.match(app.root.textContent ?? '', /full-screen or presentation.*suppress/i);
        assert.match(app.root.textContent ?? '', /does not change Windows notification settings/);
        assert.equal(app.root.querySelector<HTMLButtonElement>('#alerts-test')!.disabled, false);
        app.root.querySelector<HTMLButtonElement>('#alerts-test')!.click();
        await flush();
        assert.equal(tests, 1);
        assert.equal(app.saves.length, 0);
        assert.match(app.root.textContent ?? '', /Notification request queued/);
        assert.doesNotMatch(app.root.textContent ?? '', /Windows notification API accepted/);
    }
    finally {
        app.controller.dispose();
    }
});
test("Windows accepting state never guarantees a visible toast", async () => {
    const app = await mount({ getAlertSettings: async () => ({ ...view(), windowsState: 'accepting' }) });
    try {
        assert.match(app.root.textContent ?? '', /Windows reports it can accept notifications/);
        assert.match(app.root.textContent ?? '', /does not guarantee.*visible toast/);
    }
    finally {
        app.controller.dispose();
    }
});
test("missing Windows state remains explicitly unavailable and browser preview reports unavailable", async () => {
    const app = await mount();
    try {
        assert.match(app.root.textContent ?? '', /Windows interruption status is unavailable/);
    }
    finally {
        app.controller.dispose();
    }
    assert.equal((await previewBridge().getAlertSettings!()).windowsState, 'unavailable');
});
