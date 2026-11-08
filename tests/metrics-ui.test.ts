import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { renderMetrics } from "../src/metrics-ui.ts";
import type { MetricView } from "../src/metrics-types.ts";
test("local metrics never infer tokens, TTFT or percentiles from quota or histogram means", () => { const { document } = parseHTML("<html></html>"); Object.assign(globalThis, { document }); const view: MetricView = { settings: { enabled: false, eventRetentionDays: 30, dailyRetentionDays: 365 }, running: false, status: { coverageStartedMs: null, lastReceivedMs: null, error: null, baselinePoints: 0, duplicatePoints: 0, ignoredMetrics: 0, rejectedBatches: 0 }, history: { events: [], daily: [{ dayMs: 0, provider: "gemini", metric: "requestLatencyMs", dimension: null, model: "gemini-test", value: 300, count: 2 }], truncated: false }, endpoint: "http://127.0.0.1:43190/v1/metrics" }; const text = renderMetrics(view, null, false).textContent!; assert.match(text, /150/); assert.match(text, /Median.*unavailable/i); assert.match(text, /TTFT.*unavailable/i); assert.match(text, /local/i); assert.doesNotMatch(text, /0 tokens/); });
import { createMetricsController } from "../src/metrics-controller.ts";
import { metricPreview } from "../src/metrics-preview.ts";
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
async function mountMetrics(overrides: Record<string, unknown> = {}) { const { document } = parseHTML("<html><body><main id='root'><div id='content'></div></main></body></html>"); Object.assign(globalThis, { document }); const root = document.querySelector("#root") as unknown as HTMLElement, content = root.querySelector<HTMLElement>("#content")!; const api = metricPreview(); const calls: boolean[] = []; const bridge = { preview: true, ...api, clearMetricHistory: async (confirmed: boolean) => { calls.push(confirmed); await api.clearMetricHistory(confirmed); }, ...overrides }; const controller = createMetricsController(root, content, bridge); content.append(controller.element); await controller.start(); return { root, content, controller, calls }; }
test("telemetry clear is confirmed, cancel sends nothing and errors remain visible", async () => { const app = await mountMetrics({ clearMetricHistory: async () => { throw Error("private backend details"); } }); try {
    app.root.querySelector<HTMLButtonElement>("#metrics-clear")!.click();
    app.root.querySelector<HTMLButtonElement>("#metrics-cancel")!.click();
    assert.equal(app.root.querySelector('[role="dialog"]'), null);
    assert.deepEqual(app.calls, []);
    app.root.querySelector<HTMLButtonElement>("#metrics-clear")!.click();
    app.root.querySelector<HTMLButtonElement>("#metrics-confirm")!.click();
    await flush();
    assert.match(app.root.textContent!, /Telemetry could not be cleared/);
    assert.doesNotMatch(app.root.textContent!, /private backend/);
}
finally {
    app.controller.dispose();
} });
test("telemetry preview opens no collector and a confirmed clear removes synthetic observations", async () => { const app = await mountMetrics(); try {
    assert.match(app.root.textContent!, /Synthetic preview only/);
    assert.match(app.root.textContent!, /Collector disabled/);
    app.root.querySelector<HTMLButtonElement>("#metrics-clear")!.click();
    app.root.querySelector<HTMLButtonElement>("#metrics-confirm")!.click();
    await flush();
    assert.deepEqual(app.calls, [true]);
    assert.match(app.root.textContent!, /No local metrics/);
}
finally {
    app.controller.dispose();
} });
