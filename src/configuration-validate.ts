import type { AppConfiguration, ConfigurationError, DisplaySettings } from "./configuration-types.ts";
import type { ModelPrice, PriceRates, CacheWriteTtl } from "./pricing-types.ts";
export const MAX_CONFIGURATION_BYTES = 128 * 1024;
export class ConfigurationInputError extends Error {
    readonly code: ConfigurationError;
    constructor(code: ConfigurationError) { super(code); this.code = code; }
}
function invalid(): never { throw new ConfigurationInputError("invalidConfiguration"); }
function record(value: unknown, keys: string[]): Record<string, unknown> { if (!value || typeof value !== "object" || Array.isArray(value))
    return invalid(); const result = value as Record<string, unknown>; if (Object.keys(result).length !== keys.length || keys.some(key => !Object.hasOwn(result, key)))
    return invalid(); return result; }
function bool(value: unknown): boolean { if (typeof value !== "boolean")
    return invalid(); return value; }
function choice<T extends number | string>(value: unknown, choices: readonly T[]): T { if (!choices.includes(value as T))
    return invalid(); return value as T; }
function integer(value: unknown, min = 0): number { if (typeof value !== "number" || !Number.isSafeInteger(value) || value < min)
    return invalid(); return value; }
function text(value: unknown, pattern: RegExp): string { if (typeof value !== "string" || !pattern.test(value))
    return invalid(); return value; }
function rate(value: unknown): number | null { if (value === null)
    return null; if (typeof value !== "number" || !Number.isFinite(value) || value < 0)
    return invalid(); return value; }
function rates(value: unknown): PriceRates { const input = record(value, ["freshInputUsdPerMillion", "cacheReadUsdPerMillion", "cacheWrite", "outputUsdPerMillion"]); if (!Array.isArray(input.cacheWrite))
    return invalid(); const seen = new Set<string>(); const cacheWrite = input.cacheWrite.map(value => { const item = record(value, ["ttl", "usdPerMillion"]); const ttl = choice<CacheWriteTtl>(item.ttl, ["fiveMinutes", "oneHour", "thirtyMinutes", "twelveHours"]); if (seen.has(ttl))
    return invalid(); seen.add(ttl); return { ttl, usdPerMillion: rate(item.usdPerMillion) }; }); return { freshInputUsdPerMillion: rate(input.freshInputUsdPerMillion), cacheReadUsdPerMillion: rate(input.cacheReadUsdPerMillion), cacheWrite, outputUsdPerMillion: rate(input.outputUsdPerMillion) }; }
function modelPrice(value: unknown): ModelPrice { const input = record(value, ["modelId", "revision", "retrievedOn", "sourceUrls", "longContextThresholdTokens", "shortContext", "longContext"]); const modelId = text(input.modelId, /^[A-Za-z0-9_./:-]{1,128}$/); const revision = text(input.revision, /^[A-Za-z0-9_.-]{1,80}$/); const retrievedOn = text(input.retrievedOn, /^[0-9]{4}-[0-9]{2}-[0-9]{2}$/); const date = new Date(`${retrievedOn}T00:00:00Z`); if (retrievedOn.startsWith("0000") || !Number.isFinite(date.getTime()) || date.toISOString().slice(0, 10) !== retrievedOn)
    return invalid(); if (!Array.isArray(input.sourceUrls))
    return invalid(); const sourceUrls = input.sourceUrls.map(value => { if (typeof value !== "string" || !value.startsWith("https://") || new TextEncoder().encode(value).length > 512 || /\s/.test(value))
    return invalid(); return value; }); const threshold = input.longContextThresholdTokens === null ? null : integer(input.longContextThresholdTokens, 1); const shortContext = rates(input.shortContext), longContext = rates(input.longContext); if (threshold === null && JSON.stringify(shortContext) !== JSON.stringify(longContext))
    return invalid(); return { modelId, revision, retrievedOn, sourceUrls, longContextThresholdTokens: threshold, shortContext, longContext }; }
export function validateDisplaySettings(value: unknown): DisplaySettings { const outer = record(value, ["primaryBuckets"]); const buckets = record(outer.primaryBuckets, ["claude", "codex"]); const bucket = (value: unknown): string | null => { if (value === null)
    return null; if (typeof value !== "string" || value.length === 0 || new TextEncoder().encode(value).length > 138 || /[\u0000-\u001F\u007F-\u009F]/.test(value))
    return invalid(); return value; }; return { primaryBuckets: { claude: bucket(buckets.claude), codex: bucket(buckets.codex) } }; }
export function validateConfiguration(value: unknown): AppConfiguration {
    if (new TextEncoder().encode(JSON.stringify(value)).length > MAX_CONFIGURATION_BYTES)
        throw new ConfigurationInputError("tooLarge");
    const input = record(value, ["schemaVersion", "monitoring", "alerts", "telemetry", "quotaRetentionDays", "display", "priceOverrides"]);
    if (input.schemaVersion !== 1)
        throw new ConfigurationInputError("unsupportedSchema");
    const monitor = record(input.monitoring, ["paused", "intervalMinutes"]);
    const monitoring = { paused: bool(monitor.paused), intervalMinutes: choice(monitor.intervalMinutes, [5, 10, 15, 30] as const) };
    const alert = record(input.alerts, ["enabled", "threshold80", "threshold95", "nearReset", "expectedReset", "restored", "errors", "quietHours"]);
    let quietHours = null;
    if (alert.quietHours !== null) {
        const quiet = record(alert.quietHours, ["startMinutes", "endMinutes"]);
        const startMinutes = integer(quiet.startMinutes), endMinutes = integer(quiet.endMinutes);
        if (startMinutes >= 1440 || endMinutes >= 1440 || startMinutes === endMinutes)
            return invalid();
        quietHours = { startMinutes, endMinutes };
    }
    const alerts = { enabled: bool(alert.enabled), threshold80: bool(alert.threshold80), threshold95: bool(alert.threshold95), nearReset: bool(alert.nearReset), expectedReset: bool(alert.expectedReset), restored: bool(alert.restored), errors: bool(alert.errors), quietHours };
    const metric = record(input.telemetry, ["enabled", "eventRetentionDays", "dailyRetentionDays"]);
    const telemetry = { enabled: bool(metric.enabled), eventRetentionDays: choice(metric.eventRetentionDays, [7, 30, 90]), dailyRetentionDays: choice(metric.dailyRetentionDays, [30, 90, 365]) };
    if (telemetry.dailyRetentionDays < telemetry.eventRetentionDays)
        return invalid();
    if (!Array.isArray(input.priceOverrides))
        return invalid();
    if (new TextEncoder().encode(JSON.stringify(input.priceOverrides)).length > 64 * 1024)
        throw new ConfigurationInputError("tooLarge");
    const priceOverrides = input.priceOverrides.map(modelPrice);
    if (new Set(priceOverrides.map(price => price.modelId)).size !== priceOverrides.length)
        return invalid();
    return { schemaVersion: 1, monitoring, alerts, telemetry, quotaRetentionDays: choice(input.quotaRetentionDays, [7, 30, 90, 180, 365]), display: validateDisplaySettings(input.display), priceOverrides };
}
export async function parseConfigurationFile(file: File): Promise<AppConfiguration> { if (!/\.json$/i.test(file.name) || !["", "application/json", "text/json"].includes(file.type))
    return invalid(); if (file.size > MAX_CONFIGURATION_BYTES)
    throw new ConfigurationInputError("tooLarge"); const content = await file.text(); if (new TextEncoder().encode(content).length > MAX_CONFIGURATION_BYTES)
    throw new ConfigurationInputError("tooLarge"); let value: unknown; try {
    value = JSON.parse(content);
}
catch {
    return invalid();
} return validateConfiguration(value); }
export function configurationError(error: unknown): string { const code = error instanceof ConfigurationInputError ? error.code : error; const messages: Record<ConfigurationError, string> = { confirmationRequired: "Confirm the reviewed import before replacing settings.", unsupportedSchema: "This configuration schema is unsupported. No import was sent.", invalidConfiguration: "This configuration is invalid or contains unknown fields. Review the file before importing.", tooLarge: "The configuration exceeds 128 KiB or its manual prices exceed 64 KiB.", storageUnavailable: "Saved settings are unavailable. Reload or export to verify the current configuration.", storageCorrupt: "Saved settings could not be read safely. They have been preserved.", runtimeUnavailable: "The settings were saved, but a runtime feature could not be reconciled. Reload the affected panels or export saved settings to verify the result." }; return typeof code === "string" && Object.hasOwn(messages, code) ? messages[code as ConfigurationError] : "The configuration result is unavailable. Reload or export saved settings to verify whether changes were applied."; }
