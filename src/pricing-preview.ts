import type { BillingTokens, ModelPrice, MoneyEstimate, PricingBridge, PricingContext, PricingView } from "./pricing-types.ts";
import { exactModelId } from "./pricing-inputs.ts";
// Explicitly supplied prices only. This browser simulation never fetches rates,
// reads provider state or writes native preferences.
export function previewPricing(prices: ModelPrice[]): PricingBridge {
    const bundled = new Map(prices.map(price => [price.modelId, structuredClone(price)]));
    const overrides = new Map<string, ModelPrice>();
    function view(): PricingView { const all = new Map(bundled); for (const [id, price] of overrides)
        all.set(id, price); return structuredClone({ prices: [...all.values()], overrides: [...overrides.values()] }); }
    return { preview: true, getPricing: async () => view(), setPriceOverride: async (price) => { if (exactModelId(price.modelId) !== price.modelId)
            throw new Error('Invalid exact model ID'); overrides.set(price.modelId, structuredClone(price)); return view(); }, removePriceOverride: async (modelId) => { exactModelId(modelId); overrides.delete(modelId); return view(); }, estimateCost: async (modelId: string, tokens: BillingTokens, context: PricingContext): Promise<MoneyEstimate> => {
            if (exactModelId(modelId) !== modelId || [tokens.freshInput, tokens.cacheRead, tokens.cacheWrite, tokens.output, tokens.reasoning].some(value => value !== null && (!Number.isSafeInteger(value) || value < 0)))
                throw new Error('Invalid synthetic pricing input');
            if (tokens.outputIncludesReasoning === true && tokens.output !== null && tokens.reasoning !== null && tokens.reasoning > tokens.output)
                throw new Error('Invalid synthetic reasoning accounting');
            const result: MoneyEstimate = { modelId, label: 'apiEquivalentEstimate', currency: 'USD', basis: 'standardApiReference', priceRevision: null, retrievedOn: null, sourceUrls: [], manualOverride: overrides.has(modelId), context: { ...context }, reasoningIncludedInOutput: tokens.outputIncludesReasoning, usd: null, components: [], unavailable: null };
            const price = overrides.get(modelId) ?? bundled.get(modelId);
            if (!price) {
                result.unavailable = 'unknownModel';
                return result;
            }
            result.priceRevision = price.revision;
            result.retrievedOn = price.retrievedOn;
            result.sourceUrls = [...price.sourceUrls];
            if (price.longContextThresholdTokens !== null && context.inputContext === null) {
                result.unavailable = 'missingContext';
                return result;
            }
            const rates = context.inputContext === 'long' ? price.longContext : price.shortContext;
            if (tokens.outputIncludesReasoning === null) {
                result.unavailable = 'missingAccounting';
                return result;
            }
            if (tokens.cacheWrite !== null && tokens.cacheWrite > 0 && context.cacheWriteTtl === null) {
                result.unavailable = 'missingCacheTtl';
                return result;
            }
            const write = rates.cacheWrite.find(value => value.ttl === context.cacheWriteTtl)?.usdPerMillion ?? null;
            const components: [
                string,
                number | null,
                number | null
            ][] = [['freshInput', tokens.freshInput, rates.freshInputUsdPerMillion], ['cacheRead', tokens.cacheRead, rates.cacheReadUsdPerMillion], ['cacheWrite', tokens.cacheWrite, write], ['output', tokens.output, rates.outputUsdPerMillion]];
            if (!tokens.outputIncludesReasoning)
                components.push(['reasoning', tokens.reasoning, rates.outputUsdPerMillion]);
            for (const [tokenClass, count, rate] of components) {
                const amount = count === 0 ? 0 : count === null || rate === null ? null : count / 1000000 * rate;
                if (amount !== null && !Number.isFinite(amount))
                    throw new Error('Synthetic estimate overflow');
                result.components.push({ class: tokenClass, tokens: count, usdPerMillion: rate, usd: amount });
            }
            if (result.components.some(part => part.tokens === null))
                result.unavailable = 'missingTokens';
            else if (result.components.some(part => part.usd === null))
                result.unavailable = 'missingPrice';
            else {
                const total = result.components.reduce((sum, part) => sum + part.usd!, 0);
                if (!Number.isFinite(total))
                    throw new Error('Synthetic estimate overflow');
                result.usd = total;
            }
            return result;
        } };
}
