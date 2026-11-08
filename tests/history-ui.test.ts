import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { mountDashboard } from "../src/controller.ts";
import type { Bridge, DashboardState } from "../src/types.ts";
import type { HistoryView } from "../src/history-types.ts";
import { previewBridge } from "../src/preview.ts";
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
const now = Date.now();
function view(rangeHours: 24 | 168 | 720 = 24): HistoryView {
    return { rangeHours, fromMs: now - rangeHours * 3600000, untilMs: now, retentionDays: 90, truncated: false, error: null, checks: [], samples: [
            { agentId: "codex", bucketId: "primary", observedMs: now - 600000, usedPercent: 10, windowMinutes: 300, resetsAtMs: now + 3600000, source: "codexAppServer" },
            { agentId: "codex", bucketId: "primary", observedMs: now - 300000, usedPercent: 20, windowMinutes: 300, resetsAtMs: now + 3600000, source: "codexAppServer" }
        ] };
}
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes; }); return { promise, resolve }; }
async function mount(overrides: Partial<Bridge> = {}) {
    const { document } = parseHTML("<html><body><main id='root'></main></body></html>");
    Object.defineProperty(globalThis, "document", { value: document, configurable: true });
    const state: DashboardState = { schemaVersion: 1, agents: [], refreshing: false, lastRefreshMs: null, storageError: null, collectionError: null };
    const calls: number[] = [];
    const clears: boolean[] = [];
    const retentions: number[] = [];
    let listener: (state: DashboardState) => void = () => { };
    let refreshes = 0;
    const bridge: Bridge = { preview: false, getState: async () => state, refresh: async () => { refreshes++; return state; }, subscribe: async (fn) => { listener = fn; return () => { }; }, getHistory: async (range) => { calls.push(range); return view(range); }, getHistorySettings: async () => ({ retentionDays: 90 }), setHistoryRetention: async (days) => { retentions.push(days); return { retentionDays: days }; }, clearHistory: async (confirmed) => { clears.push(confirmed); }, ...overrides };
    const root = document.querySelector("#root") as unknown as HTMLElement;
    const controller = await mountDashboard(root, bridge);
    await flush();
    return { root, controller, calls, clears, retentions, event: () => listener(state), refreshes: () => refreshes };
}
function change(root: HTMLElement, id: string, value: string) { const select = root.querySelector<HTMLSelectElement>(`#${id}`)!; Array.from(select.options).forEach(option => { option.selected = false; }); select.querySelector<HTMLOptionElement>(`option[value="${value}"]`)!.selected = true; select.dispatchEvent(new select.ownerDocument.defaultView!.Event("change")); }
test("history starts at 24h, exposes accessible original observations and changes bounded ranges", async () => {
    const app = await mount();
    try {
        assert.deepEqual(app.calls, [24]);
        assert.ok(app.root.querySelector('svg[role="img"]'));
        assert.match(app.root.textContent ?? "", /codexAppServer/);
        assert.match(app.root.textContent ?? "", /last reported sample/i);
        assert.equal(app.root.querySelectorAll(".history-chart polyline").length, 1);
        change(app.root, "history-range", "168");
        await flush();
        change(app.root, "history-range", "720");
        await flush();
        assert.deepEqual(app.calls, [24, 168, 720]);
        assert.equal(app.refreshes(), 0);
    }
    finally {
        app.controller.dispose();
    }
});
test("chart does not join reset changes, failures, unknown percentages or long gaps", async () => {
    for (const kind of ["reset", "failure", "unavailable", "unknown", "duration", "unknownReset", "missingBucket", "gap"]) {
        const data = view();
        if (kind === "reset")
            data.samples[1]!.resetsAtMs = now + 7200000;
        if (kind === "failure" || kind === "unavailable")
            data.checks = [{ agentId: "codex", observedMs: now - 450000, status: kind === "failure" ? "error" : "unavailable", diagnosticCode: "quotaUnavailable" }];
        if (kind === "unknown")
            data.samples[1]!.usedPercent = null;
        if (kind === "duration")
            data.samples[1]!.windowMinutes = null;
        if (kind === "unknownReset")
            data.samples[1]!.resetsAtMs = null;
        if (kind === "missingBucket") {
            data.checks = [{ agentId: "codex", observedMs: now - 450000, status: "reported", diagnosticCode: null }];
            data.samples.push({ ...data.samples[0]!, bucketId: "another", observedMs: now - 450000 });
        }
        if (kind === "gap")
            data.samples[0]!.observedMs = now - 3600000;
        const app = await mount({ getHistory: async () => data });
        try {
            assert.equal(app.root.querySelectorAll(".history-chart polyline").length, 0, kind);
            if (kind === "unknown" || kind === "duration")
                assert.match(app.root.textContent ?? "", /Unknown/);
        }
        finally {
            app.controller.dispose();
        }
    }
});
test("older range results and disposal cannot replace a newer range", async () => {
    const pending = deferred<HistoryView>();
    const app = await mount({ getHistory: async (range) => range === 168 ? pending.promise : view(range) });
    try {
        change(app.root, "history-range", "168");
        change(app.root, "history-range", "720");
        await flush();
        pending.resolve({ ...view(168), truncated: true });
        await flush();
        assert.equal(app.root.querySelector<HTMLSelectElement>("#history-range")!.value, "720");
        assert.doesNotMatch(app.root.querySelector(".history-section")!.textContent ?? "", /Result limit reached/);
        change(app.root, "history-range", "168");
        app.controller.dispose();
        const text = app.root.textContent;
        pending.resolve(view(168));
        await flush();
        assert.equal(app.root.textContent, text);
    }
    finally {
        app.controller.dispose();
    }
});
test("empty, query failure, stored errors and truncated results stay explicit", async () => {
    const empty = view();
    empty.samples = [];
    const truncated = { ...view(), truncated: true };
    const failed = { ...empty, error: "storageCorrupt" as const };
    for (const [data, pattern] of [[empty, /No history yet/], [truncated, /Result limit reached/], [failed, /storageCorrupt/]] as const) {
        const app = await mount({ getHistory: async () => data });
        try {
            assert.match(app.root.textContent ?? "", pattern);
        }
        finally {
            app.controller.dispose();
        }
    }
    const app = await mount({ getHistory: async () => { throw new Error("SECRET"); } });
    try {
        assert.match(app.root.textContent ?? "", /History could not be loaded/);
        assert.doesNotMatch(app.root.textContent ?? "", /SECRET/);
    }
    finally {
        app.controller.dispose();
    }
});
test("retention save uses selected setting and preserves backend failures", async () => {
    const app = await mount();
    try {
        change(app.root, "history-retention", "30");
        app.root.querySelector<HTMLButtonElement>("#history-save")!.click();
        await flush();
        assert.deepEqual(app.retentions, [30]);
        assert.match(app.root.textContent ?? "", /Retention saved/);
    }
    finally {
        app.controller.dispose();
    }
    const failed = await mount({ setHistoryRetention: async () => { throw new Error("secret"); } });
    try {
        change(failed.root, "history-retention", "7");
        failed.root.querySelector<HTMLButtonElement>("#history-save")!.click();
        await flush();
        assert.match(failed.root.textContent ?? "", /Retention could not be saved/);
    }
    finally {
        failed.controller.dispose();
    }
});
test("clear cancellation makes zero deletion calls; confirmation is explicit and refreshes only history", async () => {
    const app = await mount();
    try {
        app.root.querySelector<HTMLButtonElement>("#history-clear")!.click();
        assert.match(app.root.querySelector('[role="dialog"]')!.textContent ?? "", /Future checks.*record/i);
        app.root.querySelector<HTMLButtonElement>("#history-cancel")!.click();
        assert.deepEqual(app.clears, []);
        assert.equal(app.root.querySelector('[role="dialog"]'), null);
        app.root.querySelector<HTMLButtonElement>("#history-clear")!.click();
        app.root.querySelector<HTMLButtonElement>("#history-confirm")!.click();
        await flush();
        assert.deepEqual(app.clears, [true]);
        assert.equal(app.refreshes(), 0);
        assert.match(app.root.textContent ?? "", /History cleared/);
    }
    finally {
        app.controller.dispose();
    }
});
test("dashboard event updates history without provider requests; missing bridge is unavailable", async () => {
    const app = await mount();
    try {
        app.event();
        await flush();
        assert.deepEqual(app.calls, [24, 24]);
        assert.equal(app.refreshes(), 0);
    }
    finally {
        app.controller.dispose();
    }
    const absent = await mount({ getHistory: undefined, getHistorySettings: undefined, setHistoryRetention: undefined, clearHistory: undefined });
    try {
        assert.match(absent.root.textContent ?? "", /History is unavailable in this connection/);
    }
    finally {
        absent.controller.dispose();
    }
});
test("pending history after disposal cannot mutate the view", async () => {
    const pending = deferred<HistoryView>();
    const app = await mount({ getHistory: async (range) => range === 168 ? pending.promise : view(range) });
    change(app.root, "history-range", "168");
    app.controller.dispose();
    const before = app.root.textContent;
    pending.resolve({ ...view(168), truncated: true });
    await flush();
    assert.equal(app.root.textContent, before);
});
test("clear failures remain visible and never trigger provider refresh", async () => {
    const app = await mount({ clearHistory: async (confirmed) => { assert.equal(confirmed, true); throw new Error("SECRET"); } });
    try {
        app.root.querySelector<HTMLButtonElement>("#history-clear")!.click();
        app.root.querySelector<HTMLButtonElement>("#history-confirm")!.click();
        await flush();
        assert.match(app.root.textContent ?? "", /History could not be cleared/);
        assert.doesNotMatch(app.root.textContent ?? "", /SECRET|History cleared\./);
        assert.equal(app.refreshes(), 0);
    }
    finally {
        app.controller.dispose();
    }
});
test("observation details paginate without hiding the count or treating markup as HTML", async () => {
    const data = view();
    data.samples = Array.from({ length: 101 }, (_, index) => ({ ...data.samples[0]!, observedMs: now - index * 300000, source: "<script>untrusted</script>" }));
    const app = await mount({ getHistory: async () => data });
    try {
        assert.equal(app.root.querySelectorAll(".history-table tbody tr").length, 100);
        assert.equal(app.root.querySelector("script"), null);
        app.root.querySelector<HTMLButtonElement>("#history-more")!.click();
        assert.equal(app.root.querySelectorAll(".history-table tbody tr").length, 101);
        assert.equal(app.root.querySelector("#history-more"), null);
    }
    finally {
        app.controller.dispose();
    }
});
test("preview history is synthetic memory with bounded ranges, retention and confirmed clearing", async () => {
    const bridge = previewBridge();
    for (const range of [24, 168, 720] as const) {
        const data = await bridge.getHistory!(range);
        assert.equal(data.rangeHours, range);
        assert.ok(data.samples.length > 0 && data.samples.length <= 10000);
        assert.ok(data.samples.every(sample => sample.source === "Synthetic preview"));
    }
    await assert.rejects(bridge.clearHistory!(false));
    await bridge.setHistoryRetention!(7);
    assert.deepEqual(await bridge.getHistorySettings!(), { retentionDays: 7 });
    await bridge.clearHistory!(true);
    assert.equal((await bridge.getHistory!(24)).samples.length, 0);
    await bridge.refresh();
    assert.ok((await bridge.getHistory!(24)).samples.length > 0);
});
