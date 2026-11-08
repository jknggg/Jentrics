import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { createClaudeController } from "../src/claude-controller.ts";
import type { Bridge } from "../src/types.ts";
import type { ClaudeBridgeStatus } from "../src/claude-types.ts";
import { previewBridge } from "../src/preview.ts";
import { mountDashboard } from "../src/controller.ts";
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
function status(): ClaudeBridgeStatus { return { configured: false, canSetup: true, canRestore: false, settingsPath: "C:/fixture/settings.json", capturePath: "C:/fixture/capture", command: null, lastCaptureMs: null, issue: null }; }
const plan = { confirmationId: "review-id", settingsPath: "C:/fixture/settings.json", capturePath: "C:/fixture/capture", command: "reviewed command <script>never HTML</script>" };
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes; }); return { promise, resolve }; }
async function mount(overrides: Partial<Bridge> = {}) {
    const { document } = parseHTML("<html><body><main id='root'><div id='content'></div></main></body></html>");
    Object.defineProperty(globalThis, "document", { value: document, configurable: true });
    const root = document.querySelector("#root") as unknown as HTMLElement;
    const content = root.querySelector<HTMLElement>("#content")!;
    const applies: unknown[][] = [];
    const restores: boolean[] = [];
    const bridge = { preview: false, getClaudeBridgeStatus: async () => status(), prepareClaudeBridge: async () => plan, applyClaudeBridge: async (...args: unknown[]) => { applies.push(args); return { ...status(), configured: true, canSetup: false, canRestore: true }; }, restoreClaudeBridge: async (confirmed: boolean) => { restores.push(confirmed); return status(); }, ...overrides } as Bridge;
    const controller = createClaudeController(root, content, bridge);
    content.append(controller.element);
    await controller.start();
    return { root, content, controller, applies, restores };
}
test("Claude setup is read-only until explicit confirmation and shows exact command and paths as text", async () => {
    const app = await mount();
    try {
        assert.match(app.root.textContent ?? "", /Not configured/);
        app.root.querySelector<HTMLButtonElement>("#claude-review")!.click();
        await flush();
        const dialog = app.root.querySelector('[role="dialog"]')!;
        assert.match(dialog.textContent ?? "", /C:\/fixture\/settings.json/);
        assert.match(dialog.textContent ?? "", /reviewed command <script>/);
        assert.equal(app.root.querySelector("script"), null);
        assert.match(dialog.textContent ?? "", /backup/i);
        assert.match(dialog.textContent ?? "", /No model request/i);
        assert.deepEqual(app.applies, []);
        app.root.querySelector<HTMLButtonElement>("#claude-cancel")!.click();
        assert.deepEqual(app.applies, []);
        assert.equal(app.root.querySelector('[role="dialog"]'), null);
        assert.equal(app.content.hasAttribute("inert"), false);
    }
    finally {
        app.controller.dispose();
    }
});
test("Claude confirmation applies the reviewed one-use ID exactly once", async () => {
    const pending = deferred<ClaudeBridgeStatus>();
    const calls: unknown[][] = [];
    const app = await mount({ applyClaudeBridge: async (...args) => { calls.push(args); return pending.promise; } });
    try {
        app.root.querySelector<HTMLButtonElement>("#claude-review")!.click();
        await flush();
        const confirm = app.root.querySelector<HTMLButtonElement>("#claude-confirm")!;
        confirm.click();
        confirm.click();
        assert.deepEqual(calls, [["review-id", true]]);
        assert.equal(app.root.querySelector<HTMLButtonElement>("#claude-review")!.disabled, true);
        pending.resolve({ ...status(), configured: true, canSetup: false, canRestore: true });
        await flush();
        assert.match(app.root.textContent ?? "", /Configured/);
        assert.match(app.root.textContent ?? "", /No capture yet/);
    }
    finally {
        pending.resolve(status());
        app.controller.dispose();
    }
});
test("existing status line is preserved and refuses setup", async () => {
    const app = await mount({ getClaudeBridgeStatus: async () => ({ ...status(), canSetup: false, issue: "existingStatusLine" }) });
    try {
        assert.equal(app.root.querySelector<HTMLButtonElement>("#claude-review")!.disabled, true);
        assert.match(app.root.textContent ?? "", /existing Claude status line.*preserved/);
        assert.match(app.root.textContent ?? "", /will not replace or wrap/);
    }
    finally {
        app.controller.dispose();
    }
});
test("restore has its own confirmation; cancel never sends a restore", async () => {
    const app = await mount({ getClaudeBridgeStatus: async () => ({ ...status(), configured: true, canSetup: false, canRestore: true }) });
    try {
        app.root.querySelector<HTMLButtonElement>("#claude-restore")!.click();
        assert.match(app.root.querySelector('[role="dialog"]')!.textContent ?? "", /changed since/i);
        app.root.querySelector<HTMLButtonElement>("#claude-cancel")!.click();
        assert.deepEqual(app.restores, []);
        app.root.querySelector<HTMLButtonElement>("#claude-restore")!.click();
        app.root.querySelector<HTMLButtonElement>("#claude-confirm")!.click();
        await flush();
        assert.deepEqual(app.restores, [true]);
        assert.match(app.root.textContent ?? "", /Previous Claude settings restored/);
    }
    finally {
        app.controller.dispose();
    }
});
test("fixed rejection issues are visible and raw failures are withheld", async () => {
    for (const error of ["changedSinceReview", new Error("SECRET")]) {
        const app = await mount({ applyClaudeBridge: async () => { throw error; } });
        try {
            app.root.querySelector<HTMLButtonElement>("#claude-review")!.click();
            await flush();
            app.root.querySelector<HTMLButtonElement>("#claude-confirm")!.click();
            await flush();
            assert.match(app.root.textContent ?? "", error === "changedSinceReview" ? /changed since/i : /could not be completed/);
            assert.doesNotMatch(app.root.textContent ?? "", /SECRET/);
        }
        finally {
            app.controller.dispose();
        }
    }
});
test("missing connection methods and status failures remain explicit", async () => {
    const app = await mount({ getClaudeBridgeStatus: undefined });
    try {
        assert.match(app.root.textContent ?? "", /unavailable in this connection/);
    }
    finally {
        app.controller.dispose();
    }
    const failed = await mount({ getClaudeBridgeStatus: async () => { throw new Error("SECRET"); } });
    try {
        assert.match(failed.root.textContent ?? "", /status could not be loaded/);
        assert.equal(failed.root.querySelector<HTMLButtonElement>("#claude-review")!.disabled, true);
    }
    finally {
        failed.controller.dispose();
    }
});
test("disposal during review cannot open a dialog or change settings", async () => {
    const pending = deferred<typeof plan>();
    const app = await mount({ prepareClaudeBridge: () => pending.promise });
    app.root.querySelector<HTMLButtonElement>("#claude-review")!.click();
    app.controller.dispose();
    pending.resolve(plan);
    await flush();
    assert.equal(app.root.querySelector('[role="dialog"]'), null);
    assert.deepEqual(app.applies, []);
});
test("preview setup and restore operate through the mounted dashboard and retain the synthetic disclosure", async () => {
    const { document } = parseHTML("<html><body><main id='root'></main></body></html>");
    Object.defineProperty(globalThis, "document", { value: document, configurable: true });
    const root = document.querySelector("#root") as unknown as HTMLElement;
    const bridge = previewBridge();
    const controller = await mountDashboard(root, bridge);
    try {
        root.querySelector<HTMLButtonElement>("#claude-review")!.click();
        await flush();
        assert.match(root.querySelector('[role="dialog"]')!.textContent ?? "", /Synthetic preview only/);
        root.querySelector<HTMLButtonElement>("#claude-confirm")!.click();
        await flush();
        assert.equal((await bridge.getClaudeBridgeStatus!()).configured, true);
        root.querySelector<HTMLButtonElement>("#claude-restore")!.click();
        root.querySelector<HTMLButtonElement>("#claude-confirm")!.click();
        await flush();
        assert.equal((await bridge.getClaudeBridgeStatus!()).configured, false);
    }
    finally {
        controller.dispose();
    }
});
test("saved capture status displays stale and future clock states without claiming a live check", async () => {
    for (const [lastCaptureMs, pattern] of [[Date.now() - 1800000, /stale/], [Date.now() + 1800000, /clock changed/]] as const) {
        const app = await mount({ getClaudeBridgeStatus: async () => ({ ...status(), configured: true, lastCaptureMs }) });
        try {
            assert.match(app.root.textContent ?? "", pattern);
        }
        finally {
            app.controller.dispose();
        }
    }
});
