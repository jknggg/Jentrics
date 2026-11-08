import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { createMetricsController } from "../src/metrics-controller.ts";
import type { MetricBridge, MetricView } from "../src/metrics-types.ts";
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
function view(enabled = false, value = 12): MetricView { return { settings: { enabled, eventRetentionDays: 30, dailyRetentionDays: 365 }, running: enabled, status: { coverageStartedMs: null, lastReceivedMs: null, error: null, baselinePoints: 0, duplicatePoints: 0, ignoredMetrics: 0, rejectedBatches: 0 }, history: { events: [], daily: [{ dayMs: 0, provider: 'claude', metric: 'tokens', dimension: 'input', model: 'fixture', value, count: null }], truncated: false }, endpoint: 'http://127.0.0.1:43190/v1/metrics' }; }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes; }); return { promise, resolve }; }
async function mount(overrides: Partial<MetricBridge> = {}) { const { document } = parseHTML("<html><body><main id='root'><div id='content'></div></main></body></html>"); Object.defineProperty(globalThis, 'document', { value: document, configurable: true }); const root = document.querySelector('#root') as unknown as HTMLElement; const content = root.querySelector<HTMLElement>('#content')!; const ranges: number[] = []; const bridge: MetricBridge = { preview: false, getMetrics: async (range) => { ranges.push(range); return view(); }, setMetricSettings: async (settings) => ({ ...view(settings.enabled), settings }), clearMetricHistory: async () => { }, ...overrides }; const controller = createMetricsController(root, content, bridge); content.append(controller.element); await controller.start(); return { root, controller, ranges }; }
function select(root: HTMLElement, id: string, value: string) { const field = root.querySelector<HTMLSelectElement>(`#${id}`)!; Array.from(field.options).forEach(option => { option.selected = false; }); field.querySelector<HTMLOptionElement>(`option[value="${value}"]`)!.selected = true; field.dispatchEvent(new field.ownerDocument.defaultView!.Event('change')); }
function enable(root: HTMLElement) { const field = root.querySelector<HTMLInputElement>('#metrics-enabled')!; field.checked = true; field.dispatchEvent(new field.ownerDocument.defaultView!.Event('change')); }
function save(root: HTMLElement) { const form = root.querySelector<HTMLFormElement>('#metrics-settings')!; form.dispatchEvent(new form.ownerDocument.defaultView!.Event('submit', { cancelable: true })); }
test('telemetry range controls query all bounded ranges without changing provider settings', async () => { const app = await mount(); try {
    for (const range of [1, 7, 365]) {
        select(app.root, 'metrics-range', String(range));
        await flush();
    }
    assert.deepEqual(app.ranges, [30, 1, 7, 365]);
    assert.match(app.root.textContent ?? '', /365 days/);
}
finally {
    app.controller.dispose();
} });
test('older telemetry range response cannot replace a newer selected range', async () => { const pending = deferred<MetricView>(); const app = await mount({ getMetrics: async (range) => range === 7 ? pending.promise : view(false, range === 365 ? 365 : 12) }); try {
    select(app.root, 'metrics-range', '7');
    select(app.root, 'metrics-range', '365');
    await flush();
    pending.resolve(view(false, 777));
    await flush();
    assert.equal(app.root.querySelector<HTMLSelectElement>('#metrics-range')!.value, '365');
    assert.match(app.root.textContent ?? '', /365/);
    assert.doesNotMatch(app.root.textContent ?? '', /777/);
}
finally {
    app.controller.dispose();
} });
test('background telemetry reload preserves unsaved enabled and retention choices', async () => { const app = await mount(); try {
    enable(app.root);
    select(app.root, 'metrics-event-retention', '90');
    await app.controller.reload();
    assert.equal(app.root.querySelector<HTMLInputElement>('#metrics-enabled')!.checked, true);
    assert.equal(app.root.querySelector<HTMLSelectElement>('#metrics-event-retention')!.value, '90');
}
finally {
    app.controller.dispose();
} });
test('saving settings invalidates a pending pre-save telemetry query', async () => { const pending = deferred<MetricView>(); let reads = 0; const app = await mount({ getMetrics: async () => ++reads === 1 ? view() : pending.promise }); try {
    const oldQuery = app.controller.reload();
    enable(app.root);
    save(app.root);
    await flush();
    pending.resolve(view(false, 777));
    await oldQuery;
    assert.equal(app.root.querySelector<HTMLInputElement>('#metrics-enabled')!.checked, true);
    assert.match(app.root.textContent ?? '', /Collector running/);
    assert.doesNotMatch(app.root.textContent ?? '', /777/);
}
finally {
    app.controller.dispose();
} });
test('confirmed telemetry clear invalidates old queries and failures survive background reloads', async () => { const pending = deferred<MetricView>(); let reads = 0; const app = await mount({ getMetrics: async () => ++reads === 2 ? pending.promise : view(), clearMetricHistory: async () => { throw new Error('SECRET'); } }); try {
    const oldQuery = app.controller.reload();
    app.root.querySelector<HTMLButtonElement>('#metrics-clear')!.click();
    app.root.querySelector<HTMLButtonElement>('#metrics-confirm')!.click();
    await flush();
    pending.resolve(view(false, 777));
    await oldQuery;
    assert.match(app.root.textContent ?? '', /Telemetry could not be cleared/);
    assert.doesNotMatch(app.root.textContent ?? '', /777|SECRET/);
    await app.controller.reload();
    assert.match(app.root.textContent ?? '', /Telemetry could not be cleared/);
}
finally {
    app.controller.dispose();
} });
test('successful clear reloads the selected range and disposal ignores late range responses', async () => { const pending = deferred<MetricView>(); let cleared = false; const ranges: number[] = []; const app = await mount({ getMetrics: async (range) => { ranges.push(range); return range === 365 ? pending.promise : { ...view(), history: { events: [], daily: cleared ? [] : view().history.daily, truncated: false } }; }, clearMetricHistory: async () => { cleared = true; } }); try {
    select(app.root, 'metrics-range', '1');
    await flush();
    app.root.querySelector<HTMLButtonElement>('#metrics-clear')!.click();
    app.root.querySelector<HTMLButtonElement>('#metrics-confirm')!.click();
    await flush();
    assert.deepEqual(ranges, [30, 1, 1]);
    assert.match(app.root.textContent ?? '', /No local metrics/);
    select(app.root, 'metrics-range', '365');
    app.controller.dispose();
    const text = app.root.textContent;
    pending.resolve(view(false, 777));
    await flush();
    assert.equal(app.root.textContent, text);
}
finally {
    app.controller.dispose();
} });
test('settings save remains single-flight while reloading a non-default selected range',async()=>{
    const pending=deferred<MetricView>();let writes=0;let reads=0;
    const app=await mount({getMetrics:async()=>++reads>=3?pending.promise:view(),setMetricSettings:async settings=>{writes++;return {...view(settings.enabled),settings}}});
    try{select(app.root,'metrics-range','7');await flush();enable(app.root);save(app.root);await flush();assert.equal(app.root.querySelector<HTMLButtonElement>('#metrics-save')!.disabled,true);save(app.root);assert.equal(writes,1);pending.resolve(view(true));await flush();assert.equal(app.root.querySelector<HTMLSelectElement>('#metrics-range')!.value,'7');}
    finally{pending.resolve(view(true));app.controller.dispose();}
});
