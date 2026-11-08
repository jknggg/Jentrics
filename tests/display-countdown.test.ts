import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { createDisplayController } from "../src/display-controller.ts";
import { createCountdownUpdater, updateQuotaCountdowns } from "../src/countdown.ts";
import { renderQuota } from "../src/quota.ts";
import type { DashboardState } from "../src/types.ts";
import type { DisplaySettings } from "../src/configuration-types.ts";
import { validateDisplaySettings } from "../src/configuration-validate.ts";
function dom() {
    const { document, window } = parseHTML("<html><body><main id='root'></main></body></html>");
    Object.defineProperty(globalThis, 'document', { value: document, configurable: true });
    Object.defineProperty(document.documentElement, 'clientHeight', {value: 720, configurable: true});
    // Linkedom has no layout engine; give countdown fixtures an explicit visible rectangle.
    window.HTMLElement.prototype.getBoundingClientRect = () => ({top: 100, bottom: 116}) as DOMRect;
    return document.querySelector('#root') as unknown as HTMLElement;
}
function state(): DashboardState { return { schemaVersion: 1, agents: [{ agentId: 'codex', version: 'fixture', availability: 'detected', authState: 'ready', capabilities: { discovery: true, authReadiness: true, quotas: true, usage: false, sessionStart: false }, observedMs: 1000, diagnosticCode: null, quota: { source: 'codexAppServer', observedMs: 1000, diagnosticCode: null, windows: [{ bucketId: 'primary', windowMinutes: 300, usedPercent: 10, resetsAtMs: 61000, observedMs: 1000 }, { bucketId: 'weekly', windowMinutes: 10080, usedPercent: 30, resetsAtMs: 100000, observedMs: 1000 }] } }], refreshing: false, lastRefreshMs: 1000, storageError: null, collectionError: null }; }
function change(root: HTMLElement, id: string, value: string) { const input = root.querySelector<HTMLSelectElement>(`#${id}`)!; Array.from(input.options).forEach(option => { option.selected = false; }); input.querySelector<HTMLOptionElement>(`option[value="${value}"]`)!.selected = true; input.dispatchEvent(new input.ownerDocument.defaultView!.Event('change')); }
test('primary bucket IDs preserve source Unicode, spaces and punctuation with UTF-8 byte bounds', () => { const settings = { primaryBuckets: { claude: null, codex: 'custom quota/配额' } }; assert.deepEqual(validateDisplaySettings(settings), settings); assert.equal(validateDisplaySettings({ primaryBuckets: { claude: '界'.repeat(46), codex: null } }).primaryBuckets.claude, '界'.repeat(46)); assert.throws(() => validateDisplaySettings({ primaryBuckets: { claude: '界'.repeat(47), codex: null } })); for (const id of ['', 'bad\u0000id', 'bad\u0080id'])
    assert.throws(() => validateDisplaySettings({ primaryBuckets: { claude: id, codex: null } })); });
test('primary quota selection persists both-null choices and keeps other buckets visible', async () => { const root = dom(); const current = state(); const saved: DisplaySettings[] = []; const controller = createDisplayController({ getDisplaySettings: async () => ({ primaryBuckets: { claude: null, codex: 'weekly' } }), setDisplaySettings: async (settings) => { saved.push(settings); return settings; } }, () => current, () => { }); root.append(controller.element); await controller.start(); try {
    change(root, 'display-codex', '');
    root.querySelector<HTMLButtonElement>('#display-save')!.click();
    await new Promise(resolve => setTimeout(resolve, 0));
    assert.deepEqual(saved, [{ primaryBuckets: { claude: null, codex: null } }]);
    const quota = renderQuota(current, false, { primaryBuckets: { claude: null, codex: 'weekly' } });
    assert.equal(quota.querySelectorAll('.quota-card').length, 2);
    assert.match(quota.querySelector('.quota-primary')!.textContent ?? '', /Primary/);
}
finally {
    controller.dispose();
} });
test('missing saved primary stays explicit until a deliberate selection is saved', async () => { const root = dom(); const current = state(); const controller = createDisplayController({ getDisplaySettings: async () => ({ primaryBuckets: { claude: null, codex: 'gone' } }) }, () => current, () => { }); root.append(controller.element); await controller.start(); try {
    assert.match(root.textContent ?? '', /Selected primary bucket unavailable/);
    assert.equal(root.querySelector<HTMLSelectElement>('#display-codex')!.value, 'gone');
    const quota = renderQuota(current, false, { primaryBuckets: { claude: null, codex: 'gone' } });
    assert.match(quota.textContent ?? '', /Selected primary bucket unavailable/);
    assert.equal(quota.querySelectorAll('.quota-primary').length, 0);
}
finally {
    controller.dispose();
} });
test('visible second countdown changes only its text node, keeps typing/focus and never implies replenishment', context => { context.mock.timers.enable({ apis: ['Date', 'setInterval'], now: 1000 }); const root = dom(); root.append(renderQuota(state())); const input = document.createElement('input'); input.value = 'unsaved text'; root.append(input); const card = root.querySelector('.quota-card'); const timer = createCountdownUpdater(root); try {
    const before = root.querySelector('[data-reset-countdown]')!.textContent;
    context.mock.timers.tick(1000);
    assert.notEqual(root.querySelector('[data-reset-countdown]')!.textContent, before);
    assert.equal(root.querySelector('.quota-card'), card);
    assert.equal(input.value, 'unsaved text');
    context.mock.timers.tick(60000);
    assert.match(root.querySelector('[data-reset-countdown]')!.textContent ?? '', /Waiting for fresh/);
    assert.doesNotMatch(root.textContent ?? '', /100% remaining/);
}
finally {
    timer.dispose();
} });
test('hidden document suspends countdown DOM work until visible again', context => { context.mock.timers.enable({ apis: ['Date', 'setInterval'], now: 1000 }); const root = dom(); root.append(renderQuota(state())); let hidden = true; Object.defineProperty(document, 'hidden', { get: () => hidden, configurable: true }); const timer = createCountdownUpdater(root); try {
    const before = root.querySelector('[data-reset-countdown]')!.textContent;
    context.mock.timers.tick(5000);
    updateQuotaCountdowns(root);
    assert.equal(root.querySelector('[data-reset-countdown]')!.textContent, before);
    hidden = false;
    document.dispatchEvent(new document.defaultView!.Event('visibilitychange'));
    assert.notEqual(root.querySelector('[data-reset-countdown]')!.textContent, before);
}
finally {
    timer.dispose();
    Object.defineProperty(document, 'hidden', { value: false, configurable: true });
} });

test('offscreen countdowns avoid DOM writes and catch up on the next visible tick', context => {
    context.mock.timers.enable({apis: ['Date', 'setInterval'], now: 1000});
    const root = dom(); root.append(renderQuota(state()));
    Object.defineProperty(document.documentElement, 'clientHeight', {value: 720, configurable: true});
    const spans = Array.from(root.querySelectorAll<HTMLElement>('[data-reset-countdown]'));
    let top = 800;
    for (const span of spans) span.getBoundingClientRect = () => ({top, bottom: top + 16}) as DOMRect;
    const before = spans[0].textContent;
    const timer = createCountdownUpdater(root);
    try {
        context.mock.timers.tick(5000);
        assert.equal(spans[0].textContent, before);
        top = 100;
        context.mock.timers.tick(1000);
        assert.match(spans[0].textContent!, /54s/);
        top = -20;
        const above = spans[0].textContent;
        context.mock.timers.tick(5000);
        assert.equal(spans[0].textContent, above);
    } finally {timer.dispose();}
});
