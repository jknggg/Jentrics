import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { renderMetricsTrend } from "../src/metrics-trend.ts";
import { createMetricsController } from "../src/metrics-controller.ts";
import type { MetricView } from "../src/metrics-types.ts";
function dom() { const { document } = parseHTML("<html><body><main id='root'><div id='content'></div></main></body></html>"); Object.defineProperty(globalThis, 'document', { value: document, configurable: true }); return document.querySelector('#root') as unknown as HTMLElement; }
function view(): MetricView { return { settings: { enabled: false, eventRetentionDays: 30, dailyRetentionDays: 365 }, running: false, status: { coverageStartedMs: null, lastReceivedMs: null, error: null, baselinePoints: 0, duplicatePoints: 0, ignoredMetrics: 0, rejectedBatches: 0 }, history: { events: [], daily: [{ dayMs: 0, provider: 'claude', metric: 'tokens', dimension: 'input', model: 'fixture', value: 10, count: null }, { dayMs: 172800000, provider: 'claude', metric: 'tokens', dimension: 'input', model: 'fixture', value: 20, count: null }, { dayMs: 0, provider: 'gemini', metric: 'requestLatencyMs', dimension: null, model: null, value: 300, count: 2 }, { dayMs: 86400000, provider: 'gemini', metric: 'requestLatencyMs', dimension: null, model: null, value: 0, count: null }, { dayMs: 0, provider: 'claude', metric: 'costUsd', dimension: null, model: 'fixture', value: 0.00000001, count: null }, { dayMs: 0, provider: 'gemini', metric: 'apiRequests', dimension: 'error', model: 'fixture', value: 3, count: null }], truncated: false }, endpoint: 'fixture' }; }
test('daily token charts use real reported values, accessible labels and discrete marks across missing days', () => { dom(); const result = renderMetricsTrend(view(), 'tokens'); assert.ok(result.querySelector('svg[role="img"]')); assert.match(result.textContent ?? '', /captured local/i); assert.match(result.textContent ?? '', /10/); assert.match(result.textContent ?? '', /20/); assert.equal(result.querySelectorAll('rect[data-trend-value]').length, 2); assert.equal(result.querySelectorAll('polyline,path').length, 0); assert.equal(result.querySelectorAll('tbody tr').length, 2); });
test('latency trends use sum/count mean only and null timing remains unavailable', () => { dom(); const result = renderMetricsTrend(view(), 'requestLatencyMs'); assert.match(result.textContent ?? '', /150/); assert.match(result.textContent ?? '', /Mean unavailable/); assert.match(result.textContent ?? '', /300/); assert.equal(result.querySelectorAll('rect[data-trend-value]').length, 1); assert.doesNotMatch(result.textContent ?? '', /p95.*150|TTFT.*150/); });
test('reported costs and provider request errors remain distinct from account totals and receiver failures', () => { dom(); const costs = renderMetricsTrend(view(), 'costUsd'); assert.match(costs.textContent ?? '', /USD/); assert.match(costs.textContent ?? '', /1(?:\.0+)?e-8/); const errors = renderMetricsTrend(view(), 'apiRequests'); assert.match(errors.textContent ?? '', /error/); assert.match(errors.textContent ?? '', /3/); assert.match(errors.textContent ?? '', /receiver.*separate/i); });
test('trend selector changes the numeric workflow without IPC reload or losing unsaved settings', async () => { const root = dom(); const content = root.querySelector<HTMLElement>('#content')!; let queries = 0; const controller = createMetricsController(root, content, { preview: false, getMetrics: async () => { queries++; return view(); } }); content.append(controller.element); await controller.start(); try {
    const enabled = root.querySelector<HTMLInputElement>('#metrics-enabled')!;
    enabled.checked = true;
    enabled.dispatchEvent(new enabled.ownerDocument.defaultView!.Event('change'));
    const selector = root.querySelector<HTMLSelectElement>('#metrics-trend')!;
    Array.from(selector.options).forEach(option => { option.selected = false; });
    selector.querySelector<HTMLOptionElement>('option[value="requestLatencyMs"]')!.selected = true;
    selector.dispatchEvent(new selector.ownerDocument.defaultView!.Event('change'));
    assert.equal(queries, 1);
    assert.equal(root.querySelector<HTMLInputElement>('#metrics-enabled')!.checked, true);
    assert.match(root.querySelector('.metrics-trend')!.textContent ?? '', /150/);
}
finally {
    controller.dispose();
} });
