import type { Bridge } from "./types.ts";
import type { AppConfiguration, DisplaySettings, MaintenanceStatus } from "./configuration-types.ts";
import type { ModelPrice, PricingView } from "./pricing-types.ts";
import { previewPricing } from "./pricing-preview.ts";
import { validateConfiguration, validateDisplaySettings } from "./configuration-validate.ts";
const DAY = 86400000;
// One atomic in-memory configuration owns preview preferences. No public native
// setters, provider files, receiver, notification service or database is invoked.
export function withPreviewConfiguration(base: Bridge, bundledPrices: ModelPrice[]): Bridge {
    for (const method of [base.getMonitorSettings, base.getAlertSettings, base.getMetrics, base.getHistorySettings, base.getPricing, base.getHistory, base.clearHistory, base.clearMetricHistory])
        if (!method)
            throw new Error("Preview configuration requires the existing synthetic services");
    let saved: AppConfiguration | null = null;
    let initializing: Promise<AppConfiguration> | null = null;
    let quotaBefore = 0, eventBefore = 0, dailyBefore = 0;
    let maintenance: MaintenanceStatus = { operation: null, dataCommitted: false, outcome: "neverRun", error: null, reclaimedBytes: null };
    async function current(): Promise<AppConfiguration> { if (saved)
        return saved; if (!initializing)
        initializing = (async () => { const [monitoring, alerts, telemetry, quota, prices] = await Promise.all([base.getMonitorSettings!(), base.getAlertSettings!(), base.getMetrics!(30), base.getHistorySettings!(), base.getPricing!()]); if (!monitoring.settings)
            throw new Error("Synthetic monitor settings are unavailable"); const result = validateConfiguration({ schemaVersion: 1, monitoring: monitoring.settings, alerts: alerts.settings, telemetry: telemetry.settings, quotaRetentionDays: quota.retentionDays, display: { primaryBuckets: { claude: null, codex: null } }, priceOverrides: prices.overrides }); if (saved === null)
            saved = result; return saved; })(); return initializing; }
    function retain(configuration: AppConfiguration, scope: "quota" | "metrics" | "all") { const now = Date.now(); if (scope !== "metrics")
        quotaBefore = Math.max(quotaBefore, Math.max(0, now - configuration.quotaRetentionDays * DAY)); if (scope !== "quota") {
        eventBefore = Math.max(eventBefore, Math.max(0, now - configuration.telemetry.eventRetentionDays * DAY));
        dailyBefore = Math.max(dailyBefore, Math.floor(Math.max(0, now - configuration.telemetry.dailyRetentionDays * DAY) / DAY) * DAY);
    } }
    function recordMaintenance(operation: MaintenanceStatus["operation"]) { maintenance = { operation, dataCommitted: true, outcome: "notNeeded", error: null, reclaimedBytes: null }; }
    async function prices(): Promise<PricingView> { const configuration = await current(); const values = new Map(bundledPrices.map(price => [price.modelId, price])); for (const price of configuration.priceOverrides)
        values.set(price.modelId, price); return structuredClone({ prices: [...values.values()], overrides: configuration.priceOverrides }); }
    const result: Bridge = { ...base,
        exportConfiguration: async () => structuredClone(await current()),
        importConfiguration: async (configuration, confirmed) => { if (!confirmed)
            throw "confirmationRequired"; const next = validateConfiguration(configuration); saved = next; retain(next, "all"); recordMaintenance("configurationImport"); return structuredClone(next); },
        getDisplaySettings: async () => structuredClone((await current()).display),
        setDisplaySettings: async (settings: DisplaySettings) => { const display = validateDisplaySettings(settings); saved = { ...await current(), display }; return structuredClone(display); },
        getStorageMaintenance: async () => ({ ...maintenance }),
        getDiagnosticsStatus: async () => ({ code: "notStarted", writtenRecords: 0, droppedRecords: 0 }),
        getMonitorSettings: async () => ({ ...await base.getMonitorSettings!(), settings: structuredClone((await current()).monitoring) }),
        setMonitorSettings: async (settings) => { const next = validateConfiguration({ ...await current(), monitoring: settings }); saved = next; return { ...await base.getMonitorSettings!(), settings: structuredClone(next.monitoring) }; },
        getAlertSettings: async () => ({ ...await base.getAlertSettings!(), settings: structuredClone((await current()).alerts) }),
        setAlertSettings: async (settings) => { saved = validateConfiguration({ ...await current(), alerts: settings }); return { ...await base.getAlertSettings!(), settings: structuredClone(saved.alerts) }; },
        requestAlertPermission: base.requestAlertPermission ? async () => ({ ...await base.requestAlertPermission!(), settings: structuredClone((await current()).alerts) }) : undefined,
        getHistorySettings: async () => ({ retentionDays: (await current()).quotaRetentionDays }),
        setHistoryRetention: async (retentionDays) => { saved = validateConfiguration({ ...await current(), quotaRetentionDays: retentionDays }); retain(saved, "quota"); recordMaintenance("quotaRetention"); return { retentionDays: saved.quotaRetentionDays }; },
        getHistory: async (rangeHours) => { const history = await base.getHistory!(rangeHours); const configuration = await current(); return { ...history, retentionDays: configuration.quotaRetentionDays, samples: history.samples.filter(sample => sample.observedMs >= quotaBefore), checks: history.checks.filter(check => check.observedMs >= quotaBefore) }; },
        clearHistory: async (confirmed) => { await base.clearHistory!(confirmed); recordMaintenance("quotaClear"); },
        getMetrics: async (rangeDays) => { const value = await base.getMetrics!(rangeDays); const configuration = await current(); const from = Math.max(0, Date.now() - rangeDays * DAY); return { ...value, settings: structuredClone(configuration.telemetry), running: configuration.telemetry.enabled, history: { ...value.history, events: value.history.events.filter(event => event.endMs >= Math.max(from, eventBefore)), daily: value.history.daily.filter(day => day.dayMs >= Math.max(Math.floor(from / DAY) * DAY, dailyBefore)) } }; },
        setMetricSettings: async (settings) => { saved = validateConfiguration({ ...await current(), telemetry: settings }); retain(saved, "metrics"); recordMaintenance("metricRetention"); return result.getMetrics!(30); },
        clearMetricHistory: async (confirmed) => { await base.clearMetricHistory!(confirmed); recordMaintenance("metricClear"); },
        getPricing: prices,
        setPriceOverride: async (price) => { const configuration = await current(); saved = validateConfiguration({ ...configuration, priceOverrides: [...configuration.priceOverrides.filter(existing => existing.modelId !== price.modelId), price] }); return prices(); },
        removePriceOverride: async (modelId) => { const configuration = await current(); saved = { ...configuration, priceOverrides: configuration.priceOverrides.filter(price => price.modelId !== modelId) }; return prices(); },
        estimateCost: async (modelId, tokens, context) => previewPricing((await prices()).prices).estimateCost!(modelId, tokens, context)
    };
    return result;
}
