import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { mountDashboard } from "../src/controller.ts";
import { previewBridge } from "../src/preview.ts";
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
async function mount() { const { document } = parseHTML("<html><body><main id='root'></main></body></html>"); Object.defineProperty(globalThis, 'document', { value: document, configurable: true }); const root = document.querySelector('#root') as unknown as HTMLElement; const bridge = previewBridge(); const controller = await mountDashboard(root, bridge); return { root, bridge, controller }; }
function select(root: HTMLElement, id: string, value: string) { const control = root.querySelector<HTMLSelectElement>(`#${id}`)!; Array.from(control.options).forEach(option => { option.selected = false; }); Array.from(control.options).find(option => option.value === value)!.selected = true; control.dispatchEvent(new control.ownerDocument.defaultView!.Event('change')); }
test('configuration apply refreshes saved display/settings while preserving unrelated calculator drafts', async () => { const app = await mount(); try {
    const calculator = app.root.querySelector<HTMLInputElement>('#pricing-count-output')!;
    calculator.value = '123';
    calculator.dispatchEvent(new calculator.ownerDocument.defaultView!.Event('input'));
    const incoming = await app.bridge.exportConfiguration!();
    incoming.monitoring.paused = true;
    incoming.monitoring.intervalMinutes = 15;
    incoming.alerts.enabled = true;
    incoming.telemetry.enabled = true;
    incoming.quotaRetentionDays = 7;
    incoming.display.primaryBuckets.codex = 'codex:secondary';
    const input = app.root.querySelector<HTMLInputElement>('#configuration-file')!;
    Object.defineProperty(input, 'files', { value: [new File([JSON.stringify(incoming)], 'settings.json', { type: 'application/json' })] });
    input.dispatchEvent(new input.ownerDocument.defaultView!.Event('change'));
    await flush();
    app.root.querySelector<HTMLInputElement>('#configuration-activate')!.checked = true;
    app.root.querySelector<HTMLButtonElement>('#configuration-confirm')!.click();
    await flush();
    await flush();
    assert.equal(app.root.querySelector<HTMLInputElement>('#pricing-count-output')!.value, '123');
    assert.equal(app.root.querySelector<HTMLSelectElement>('#monitor-mode')!.value, 'paused');
    assert.equal(app.root.querySelector<HTMLSelectElement>('#history-retention')!.value, '7');
    assert.equal(app.root.querySelector<HTMLInputElement>('#alerts-enabled')!.checked, true);
    assert.equal(app.root.querySelector<HTMLInputElement>('#metrics-enabled')!.checked, true);
    assert.match(app.root.querySelector('.quota-primary')!.textContent ?? '', /codex:secondary/);
}
finally {
    app.controller.dispose();
} });
test('explicit quota and telemetry clearing refresh maintenance without replacing unrelated inputs', async () => { const app = await mount(); try {
    app.root.querySelector<HTMLButtonElement>('#history-clear')!.click();
    app.root.querySelector<HTMLButtonElement>('#history-confirm')!.click();
    await flush();
    await flush();
    assert.equal((await app.bridge.getStorageMaintenance!()).operation, 'quotaClear');
    assert.match(app.root.querySelector('.maintenance-status')!.textContent ?? '', /data changes were committed/);
    app.root.querySelector<HTMLButtonElement>('#metrics-clear')!.click();
    app.root.querySelector<HTMLButtonElement>('#metrics-confirm')!.click();
    await flush();
    await flush();
    assert.equal((await app.bridge.getStorageMaintenance!()).operation, 'metricClear');
    assert.match(app.root.querySelector('.maintenance-status')!.textContent ?? '', /data changes were committed/);
}
finally {
    app.controller.dispose();
} });
