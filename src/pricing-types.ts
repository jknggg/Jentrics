export type CacheWriteTtl = "fiveMinutes" | "oneHour" | "thirtyMinutes" | "twelveHours";
export interface PriceRates {
    freshInputUsdPerMillion: number | null;
    cacheReadUsdPerMillion: number | null;
    cacheWrite: {
        ttl: CacheWriteTtl;
        usdPerMillion: number | null;
    }[];
    outputUsdPerMillion: number | null;
}
export interface ModelPrice {
    modelId: string;
    revision: string;
    retrievedOn: string;
    sourceUrls: string[];
    longContextThresholdTokens: number | null;
    shortContext: PriceRates;
    longContext: PriceRates;
}
export interface PricingView {
    prices: ModelPrice[];
    overrides: ModelPrice[];
}
export interface BillingTokens {
    freshInput: number | null;
    cacheRead: number | null;
    cacheWrite: number | null;
    output: number | null;
    reasoning: number | null;
    outputIncludesReasoning: boolean | null;
}
export interface PricingContext {
    inputContext: "short" | "long" | null;
    cacheWriteTtl: CacheWriteTtl | null;
}
export interface MoneyEstimate {
    modelId: string;
    label: string;
    currency: string;
    basis: string;
    priceRevision: string | null;
    retrievedOn: string | null;
    sourceUrls: string[];
    manualOverride: boolean;
    context: PricingContext;
    reasoningIncludedInOutput: boolean | null;
    usd: number | null;
    components: {
        class: string;
        tokens: number | null;
        usdPerMillion: number | null;
        usd: number | null;
    }[];
    unavailable: string | null;
}
export interface PricingBridge {
    preview: boolean;
    getPricing?(): Promise<PricingView>;
    setPriceOverride?(price: ModelPrice): Promise<PricingView>;
    removePriceOverride?(modelId: string): Promise<PricingView>;
    estimateCost?(modelId: string, tokens: BillingTokens, context: PricingContext): Promise<MoneyEstimate>;
}
