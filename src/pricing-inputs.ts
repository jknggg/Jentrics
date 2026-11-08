import type { BillingTokens, CacheWriteTtl, ModelPrice, PriceRates, PricingContext } from "./pricing-types.ts";
export const pricingTokenFields = [['freshInput', 'Fresh input tokens'], ['cacheRead', 'Cache-read tokens'], ['cacheWrite', 'Cache-write tokens'], ['output', 'Output tokens'], ['reasoning', 'Reasoning tokens']] as const;
export const pricingTtls: readonly (readonly [
    CacheWriteTtl,
    string
])[] = [['fiveMinutes', '5 minutes'], ['oneHour', '1 hour'], ['thirtyMinutes', '30 minutes'], ['twelveHours', '12 hours']];
export const pricingRateFields = [['input', 'Fresh input'], ['read', 'Cache read'], ['output', 'Output / reasoning']] as const;
export function initialPricingFields(): Record<string, string> { const values: Record<string, string> = { "pricing-model": "", "pricing-reasoning-mode": "", "pricing-context": "", "pricing-ttl": "", "pricing-override-model": "", "pricing-override-mode": "", "pricing-context-threshold": "", "pricing-sources": "" }; for (const [key] of pricingTokenFields)
    values[`pricing-count-${key}`] = ""; for (const context of ['short', 'long']) {
    for (const [key] of pricingRateFields)
        values[`pricing-${context}-${key}`] = "";
    for (const [ttl] of pricingTtls)
        values[`pricing-${context}-write-${ttl}`] = "";
} return values; }
export function exactModelId(value: string): string { const id = value.trim(); if (!/^[a-zA-Z0-9_./:-]{1,128}$/.test(id))
    throw new Error("Enter an exact model ID using letters, numbers, _ . / : - (maximum 128 characters)."); return id; }
function count(value: string): number | null { const text = value.trim(); if (text === "")
    return null; if (!/^\d+$/.test(text) || !Number.isSafeInteger(Number(text)))
    throw new Error("Token counts must be non-negative whole safe integers; blank means unknown."); return Number(text); }
function rate(value: string): number | null { const text = value.trim(); if (text === "")
    return null; const result = Number(text); if (!Number.isFinite(result) || result < 0)
    throw new Error("Rates must be finite non-negative USD amounts per million tokens; blank means unknown."); return result; }
export function calculatorInput(fields: Record<string, string>): {
    modelId: string;
    tokens: BillingTokens;
    context: PricingContext;
} { const modelId = exactModelId(fields['pricing-model']!); const tokens: BillingTokens = { freshInput: count(fields['pricing-count-freshInput']!), cacheRead: count(fields['pricing-count-cacheRead']!), cacheWrite: count(fields['pricing-count-cacheWrite']!), output: count(fields['pricing-count-output']!), reasoning: count(fields['pricing-count-reasoning']!), outputIncludesReasoning: fields['pricing-reasoning-mode'] === 'yes' ? true : fields['pricing-reasoning-mode'] === 'no' ? false : null }; if (tokens.outputIncludesReasoning === true && tokens.output !== null && tokens.reasoning !== null && tokens.reasoning > tokens.output)
    throw new Error("Reasoning tokens cannot exceed output when reasoning is already included in output."); const context: PricingContext = { inputContext: fields['pricing-context'] === 'short' ? 'short' : fields['pricing-context'] === 'long' ? 'long' : null, cacheWriteTtl: pricingTtls.find(([ttl]) => ttl === fields['pricing-ttl'])?.[0] ?? null }; return { modelId, tokens, context }; }
export function manualPrice(fields: Record<string, string>, now: Date): ModelPrice { const modelId = exactModelId(fields['pricing-override-model']!); const mode = fields['pricing-override-mode']; if (mode !== 'flat' && mode !== 'tiered')
    throw new Error("Choose flat rates or context-tiered rates explicitly."); const threshold = mode === 'flat' ? null : count(fields['pricing-context-threshold']!); if (mode === 'tiered' && (threshold === null || threshold === 0))
    throw new Error("Enter a positive whole long-context threshold for tiered rates."); const rates = (context: string): PriceRates => ({ freshInputUsdPerMillion: rate(fields[`pricing-${context}-input`]!), cacheReadUsdPerMillion: rate(fields[`pricing-${context}-read`]!), outputUsdPerMillion: rate(fields[`pricing-${context}-output`]!), cacheWrite: pricingTtls.map(([ttl]) => ({ ttl, usdPerMillion: rate(fields[`pricing-${context}-write-${ttl}`]!) })) }); const shortContext = rates('short'); const longContext = mode === 'flat' ? { ...shortContext, cacheWrite: shortContext.cacheWrite.map(value => ({ ...value })) } : rates('long'); const sourceUrls = fields['pricing-sources']!.split(/\r?\n/).map(value => value.trim()).filter(value => value.length > 0); if (sourceUrls.some(value => !value.startsWith('https://') || value.length > 512 || /\s/.test(value)))
    throw new Error("Source references must be HTTPS URLs, one per line (maximum 512 characters each), or left blank."); return { modelId, revision: `user-${now.getTime()}`, retrievedOn: now.toISOString().slice(0, 10), sourceUrls, longContextThresholdTokens: threshold, shortContext, longContext }; }
