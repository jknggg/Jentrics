import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { parseConfigurationFile, validateConfiguration } from "../src/configuration-validate.ts";
import { createConfigurationController } from "../src/configuration-controller.ts";
import type { AppConfiguration, ConfigurationBridge } from "../src/configuration-types.ts";
import { previewBridge } from "../src/preview.ts";
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
function configuration(): AppConfiguration { return { schemaVersion: 1, monitoring: { paused: false, intervalMinutes: 5 }, alerts: { enabled: false, threshold80: true, threshold95: true, nearReset: true, expectedReset: true, restored: true, errors: true, quietHours: null }, telemetry: { enabled: false, eventRetentionDays: 30, dailyRetentionDays: 365 }, quotaRetentionDays: 90, display: { primaryBuckets: { claude: null, codex: null } }, priceOverrides: [] }; }
function file(value: unknown, name = 'settings.json', type = 'application/json'): File { return new File([typeof value === 'string' ? value : JSON.stringify(value)], name, { type }); }
async function mount(overrides: Partial<ConfigurationBridge> = {}) { const { document } = parseHTML("<html><body><main id='root'><div id='content'></div></main></body></html>"); Object.defineProperty(globalThis, 'document', { value: document, configurable: true }); const root = document.querySelector('#root') as unknown as HTMLElement; const content = root.querySelector<HTMLElement>('#content')!; const imports: unknown[][] = []; let applied = 0; const bridge: ConfigurationBridge = { exportConfiguration: async () => configuration(), importConfiguration: async (...args) => { imports.push(args); return args[0]; }, ...overrides }; const controller = createConfigurationController(root, content, bridge, async () => { applied++; }); content.append(controller.element); await controller.start(); return { root, content, controller, imports, applied: () => applied }; }
function choose(root: HTMLElement, value: File) { const input = root.querySelector<HTMLInputElement>('#configuration-file')!; Object.defineProperty(input, 'files', { value: [value], configurable: true }); input.dispatchEvent(new input.ownerDocument.defaultView!.Event('change')); }
test('configuration file rejects future/unknown/malformed/oversized and wrong-type input before review', async () => {
    await assert.rejects(parseConfigurationFile(file({ ...configuration(), schemaVersion: 2 })), /unsupportedSchema/);
    await assert.rejects(parseConfigurationFile(file({ ...configuration(), credentials: 'secret' })), /invalidConfiguration/);
    await assert.rejects(parseConfigurationFile(file('{broken')), /invalidConfiguration/);
    await assert.rejects(parseConfigurationFile(file(' '.repeat(128 * 1024 + 1))), /tooLarge/);
    await assert.rejects(parseConfigurationFile(file(configuration(), 'picture.png', 'image/png')), /invalidConfiguration/);
    assert.throws(() => validateConfiguration({ ...configuration(), display: { primaryBuckets: { claude: null, codex: null, gemini: null } } }), /invalidConfiguration/);
});
test('configuration import summary is readable, cancellation writes nothing and activation requires explicit consent', async () => {
    const app = await mount();
    try {
        const incoming = configuration();
        incoming.alerts.enabled = true;
        incoming.telemetry.enabled = true;
        choose(app.root, file(incoming));
        await flush();
        const dialog = app.root.querySelector('[role="dialog"]')!;
        assert.ok(dialog);
        assert.match(dialog.textContent ?? '', /replace/i);
        assert.match(dialog.textContent ?? '', /Automatic alerts: Enabled/);
        assert.match(dialog.textContent ?? '', /Local telemetry collector: Enabled/);
        assert.match(dialog.textContent ?? '', /provider.*unchanged/i);
        app.root.querySelector<HTMLButtonElement>('#configuration-confirm')!.click();
        await flush();
        assert.equal(app.imports.length, 0);
        assert.ok(app.root.querySelector('[role="dialog"]'));
        app.root.querySelector<HTMLButtonElement>('#configuration-cancel')!.click();
        assert.equal(app.imports.length, 0);
        assert.equal(app.content.hasAttribute('inert'), false);
        choose(app.root, file(incoming));
        await flush();
        app.root.querySelector<HTMLInputElement>('#configuration-activate')!.checked = true;
        app.root.querySelector<HTMLButtonElement>('#configuration-confirm')!.click();
        await flush();
        assert.equal(app.imports.length, 1);
        assert.equal(app.imports[0]![1], true);
        assert.equal(app.applied(), 1);
    }
    finally {
        app.controller.dispose();
    }
});
test('runtime reconciliation failure discloses persisted settings and never says nothing changed', async () => { const app = await mount({ importConfiguration: async () => { throw 'runtimeUnavailable'; } }); try {
    choose(app.root, file(configuration()));
    await flush();
    app.root.querySelector<HTMLButtonElement>('#configuration-confirm')!.click();
    await flush();
    assert.match(app.root.textContent ?? '', /settings were saved/i);
    assert.match(app.root.textContent ?? '', /runtime/i);
    assert.doesNotMatch(app.root.textContent ?? '', /nothing changed|unchanged settings/i);
    assert.equal(app.applied(), 1);
}
finally {
    app.controller.dispose();
} });
test('older file reads and disposal cannot replace a newer import review', async () => { let resolve!: (value: string) => void; const pending = new Promise<string>(yes => { resolve = yes; }); const old = file(configuration()); Object.defineProperty(old, 'text', { value: () => pending }); const app = await mount(); try {
    choose(app.root, old);
    const newest = configuration();
    newest.monitoring.intervalMinutes = 30;
    choose(app.root, file(newest));
    await flush();
    resolve(JSON.stringify(configuration()));
    await flush();
    assert.match(app.root.querySelector('[role="dialog"]')!.textContent ?? '', /30 minutes/);
    app.controller.dispose();
    assert.equal(app.root.querySelector('[role="dialog"]'), null);
}
finally {
    app.controller.dispose();
} });
test('configuration export downloads only validated app-owned settings and never imports on load', async () => { const created: Blob[] = []; const original = URL.createObjectURL; URL.createObjectURL = blob => { if (!(blob instanceof Blob))
    throw new Error("Expected configuration Blob"); created.push(blob); return 'blob:fixture'; }; const originalRevoke = URL.revokeObjectURL; URL.revokeObjectURL = () => { }; const app = await mount(); try {
    assert.equal(app.imports.length, 0);
    app.root.querySelector<HTMLButtonElement>('#configuration-export')!.click();
    await flush();
    assert.equal(created.length, 1);
    assert.deepEqual(JSON.parse(await created[0]!.text()), configuration());
    assert.match(app.root.textContent ?? '', /manual price.*metadata/i);
}
finally {
    app.controller.dispose();
    URL.createObjectURL = original;
    URL.revokeObjectURL = originalRevoke;
} });
test('synthetic configuration import is atomic, cancellation changes nothing and all affected preview panels share the saved settings', async () => { const bridge = previewBridge(); const before = await bridge.exportConfiguration!(); const incoming = structuredClone(before); incoming.monitoring.paused = true; incoming.monitoring.intervalMinutes = 15; incoming.alerts.enabled = true; incoming.telemetry.enabled = true; incoming.quotaRetentionDays = 7; incoming.display.primaryBuckets.codex = 'codex:secondary'; await assert.rejects(bridge.importConfiguration!(incoming, false)); assert.deepEqual(await bridge.exportConfiguration!(), before); await bridge.importConfiguration!(incoming, true); assert.deepEqual(await bridge.exportConfiguration!(), incoming); assert.equal((await bridge.getMonitorSettings!()).settings!.paused, true); assert.equal((await bridge.getAlertSettings!()).settings.enabled, true); assert.equal((await bridge.getMetrics!(7)).settings.enabled, true); assert.equal((await bridge.getHistory!(24)).retentionDays, 7); assert.equal((await bridge.getDisplaySettings!()).primaryBuckets.codex, 'codex:secondary'); assert.equal((await bridge.getAdapterCatalog!()).interfaces.length, 6); assert.ok((await bridge.getAdapterCatalog!()).interfaces.every(source => source.cliVersion === null && !source.nativeCollectionAllowed)); });
test('configuration rejects duplicate manual models, unknown nested fields and UTF-8 oversize without partial preview changes', async () => { const bridge = previewBridge(); const before = await bridge.exportConfiguration!(); const incoming = structuredClone(before); const price = (await bridge.getPricing!()).prices[0]!; incoming.priceOverrides = [price, price]; await assert.rejects(bridge.importConfiguration!(incoming, true)); assert.deepEqual(await bridge.exportConfiguration!(), before); assert.throws(() => validateConfiguration({ ...before, monitoring: { ...before.monitoring, executable: 'not allowed' } }), /invalidConfiguration/); const large = file('界'.repeat(50000)); Object.defineProperty(large, 'size', { value: 0 }); await assert.rejects(parseConfigurationFile(large), /tooLarge/); });
test('a pre-import saved-settings query cannot replace the imported configuration', async () => { let resolve!: (value: AppConfiguration) => void; let reads = 0; const pending = new Promise<AppConfiguration>(yes => { resolve = yes; }); const app = await mount({ exportConfiguration: async () => ++reads === 2 ? pending : configuration() }); try {
    const old = app.controller.reload();
    const incoming = configuration();
    incoming.monitoring.intervalMinutes = 30;
    choose(app.root, file(incoming));
    await flush();
    app.root.querySelector<HTMLButtonElement>('#configuration-confirm')!.click();
    await flush();
    resolve(configuration());
    await old;
    assert.match(app.root.querySelector('.configuration-summary')!.textContent ?? '', /30 minutes/);
}
finally {
    app.controller.dispose();
} });
test('a valid near-limit configuration export remains within the import byte limit', async () => { const value = configuration(); const rates = { freshInputUsdPerMillion: null, cacheReadUsdPerMillion: null, outputUsdPerMillion: null, cacheWrite: (['fiveMinutes', 'oneHour', 'thirtyMinutes', 'twelveHours'] as const).map(ttl => ({ ttl, usdPerMillion: null })) }; for (let index = 0; index < 1000; index++) {
    const price = { modelId: `fixture-${index}`, revision: 'fixture-1', retrievedOn: '2026-10-31', sourceUrls: [], longContextThresholdTokens: null, shortContext: rates, longContext: rates };
    value.priceOverrides.push(price);
    if (new TextEncoder().encode(JSON.stringify(value.priceOverrides)).length > 65520) {
        value.priceOverrides.pop();
        break;
    }
} const created: Blob[] = []; const oldCreate = URL.createObjectURL, oldRevoke = URL.revokeObjectURL; URL.createObjectURL = blob => { if (!(blob instanceof Blob))
    throw new Error('Expected Blob'); created.push(blob); return 'blob:fixture'; }; URL.revokeObjectURL = () => { }; const app = await mount({ exportConfiguration: async () => value }); try {
    app.root.querySelector<HTMLButtonElement>('#configuration-export')!.click();
    await flush();
    assert.ok(created[0]!.size <= 128 * 1024);
    assert.deepEqual(await parseConfigurationFile(new File([created[0]!], 'settings.json', { type: 'application/json' })), value);
}
finally {
    app.controller.dispose();
    URL.createObjectURL = oldCreate;
    URL.revokeObjectURL = oldRevoke;
} });
