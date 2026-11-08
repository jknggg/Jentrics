import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { createPricingController } from "../src/pricing-controller.ts";
import type { PricingBridge, ModelPrice, MoneyEstimate, BillingTokens, PricingContext } from "../src/pricing-types.ts";
import { previewPricing } from "../src/pricing-preview.ts";
import { previewBridge } from "../src/preview.ts";
import { mountDashboard } from "../src/controller.ts";
import bundledRegistry from "../src-tauri/src/pricing/registry.json" with {type:"json"};
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
function price(): ModelPrice { const rates = { freshInputUsdPerMillion: 2, cacheReadUsdPerMillion: 0.1, cacheWrite: [{ ttl: "fiveMinutes" as const, usdPerMillion: 3 }], outputUsdPerMillion: 10 }; return { modelId: "fixture-model", revision: "fixture-1", retrievedOn: "2026-10-30", sourceUrls: ["https://fixture.example/prices"], longContextThresholdTokens: null, shortContext: rates, longContext: rates }; }
function estimate(): MoneyEstimate { return { modelId: "fixture-model", label: "apiEquivalentEstimate", currency: "USD", basis: "standardApiReference", priceRevision: "fixture-1", retrievedOn: "2026-10-30", sourceUrls: ["https://fixture.example/prices"], manualOverride: false, context: { inputContext: null, cacheWriteTtl: null }, reasoningIncludedInOutput: true, usd: null, components: [], unavailable: "missingTokens" }; }
async function mount(overrides: Partial<PricingBridge> = {}) { const { document } = parseHTML("<html><body><main id='root'></main></body></html>"); Object.defineProperty(globalThis, "document", { value: document, configurable: true }); const calls: {
    model: string;
    tokens: BillingTokens;
    context: PricingContext;
}[] = []; const saves: ModelPrice[] = []; const removes: string[] = []; const bridge: PricingBridge = { preview: false, getPricing: async () => ({ prices: [price()], overrides: [] }), estimateCost: async (model, tokens, context) => { calls.push({ model, tokens, context }); return estimate(); }, setPriceOverride: async (value) => { saves.push(value); return { prices: [value], overrides: [value] }; }, removePriceOverride: async (id) => { removes.push(id); return { prices: [price()], overrides: [] }; }, ...overrides }; const root = document.querySelector("#root") as unknown as HTMLElement; const controller = createPricingController(bridge); root.append(controller.element); await controller.start(); return { root, controller, calls, saves, removes }; }
function input(root: HTMLElement, id: string, value: string) { const field = root.querySelector<HTMLInputElement | HTMLSelectElement>(`#${id}`)!; if (field.tagName === "SELECT") {
    const select = field as HTMLSelectElement;
    Array.from(select.options).forEach(option => { option.selected = false; });
    select.querySelector<HTMLOptionElement>(`option[value="${value}"]`)!.selected = true;
}
else
    field.value = value; field.dispatchEvent(new field.ownerDocument.defaultView!.Event(field.tagName === "SELECT" ? "change" : "input")); }
function submit(root: HTMLElement, id: string) { const form = root.querySelector<HTMLFormElement>(`#${id}`)!; form.dispatchEvent(new form.ownerDocument.defaultView!.Event("submit", { cancelable: true })); }
test("calculator preserves blank unknown counts and explicit zero, with exact context/accounting choices", async () => { const app = await mount(); try {
    assert.match(app.root.textContent ?? "", /API-equivalent estimate/);
    assert.match(app.root.textContent ?? "", /not.*subscription bill/i);
    assert.match(app.root.textContent ?? "", /2026-10-30/);
    input(app.root, "pricing-model", "fixture-model");
    input(app.root, "pricing-count-freshInput", "0");
    input(app.root, "pricing-reasoning-mode", "yes");
    input(app.root, "pricing-context", "long");
    input(app.root, "pricing-ttl", "oneHour");
    submit(app.root, "pricing-calculator");
    await flush();
    assert.equal(app.calls[0]!.tokens.freshInput, 0);
    assert.equal(app.calls[0]!.tokens.cacheRead, null);
    assert.equal(app.calls[0]!.tokens.outputIncludesReasoning, true);
    assert.deepEqual(app.calls[0]!.context, { inputContext: "long", cacheWriteTtl: "oneHour" });
    assert.match(app.root.textContent ?? "", /Token counts are missing/);
}
finally {
    app.controller.dispose();
} });
test("invalid negative, fractional and unsafe token counts never invoke calculation", async () => { for (const value of ["-1", "1.5", "9007199254740992"]) {
    const app = await mount();
    try {
        input(app.root, "pricing-model", "fixture-model");
        input(app.root, "pricing-count-output", value);
        submit(app.root, "pricing-calculator");
        assert.equal(app.calls.length, 0);
        assert.match(app.root.textContent ?? "", /whole.*safe|safe.*whole/i);
    }
    finally {
        app.controller.dispose();
    }
} });
test("null estimates stay unavailable and tiny positive USD never becomes a fake zero", async () => { const app = await mount({ estimateCost: async () => ({ ...estimate(), usd: 0.00000000001, unavailable: null, components: [{ class: "output", tokens: 1, usdPerMillion: 0.00001, usd: 0.00000000001 }] }) }); try {
    input(app.root, "pricing-model", "fixture-model");
    submit(app.root, "pricing-calculator");
    await flush();
    assert.match(app.root.textContent ?? "", /USD 1(?:\.0+)?e-11/);
    assert.doesNotMatch(app.root.textContent ?? "", /USD 0(?:\.0+)?\b/);
}
finally {
    app.controller.dispose();
} });
test("manual flat override requires explicit mode, preserves unknown rates, and only saves on submit", async () => { const app = await mount(); try {
    input(app.root, "pricing-override-model", "my/model:exact");
    input(app.root, "pricing-short-input", "2.5");
    submit(app.root, "pricing-override");
    assert.equal(app.saves.length, 0);
    assert.match(app.root.textContent ?? "", /Choose.*flat.*tiered/i);
    input(app.root, "pricing-override-mode", "flat");
    submit(app.root, "pricing-override");
    await flush();
    const saved = app.saves[0]!;
    assert.equal(saved.modelId, "my/model:exact");
    assert.equal(saved.shortContext.freshInputUsdPerMillion, 2.5);
    assert.equal(saved.shortContext.outputUsdPerMillion, null);
    assert.equal(saved.longContextThresholdTokens, null);
    assert.deepEqual(saved.shortContext, saved.longContext);
    assert.deepEqual(saved.sourceUrls, []);
    assert.match(saved.revision, /^user-\d+$/);
    assert.equal(saved.retrievedOn, new Date().toISOString().slice(0, 10));
}
finally {
    app.controller.dispose();
} });
test("tiered override validates threshold/rates/sources and saves four explicit cache TTL classes", async () => { const app = await mount(); try {
    input(app.root, "pricing-override-model", "custom");
    input(app.root, "pricing-override-mode", "tiered");
    submit(app.root, "pricing-override");
    assert.equal(app.saves.length, 0);
    input(app.root, "pricing-context-threshold", "1000");
    input(app.root, "pricing-short-input", "-2");
    submit(app.root, "pricing-override");
    assert.equal(app.saves.length, 0);
    input(app.root, "pricing-short-input", "1");
    input(app.root, "pricing-long-input", "2");
    input(app.root, "pricing-short-write-oneHour", "0");
    input(app.root, "pricing-sources", "javascript:alert(1)");
    submit(app.root, "pricing-override");
    assert.equal(app.saves.length, 0);
    input(app.root, "pricing-sources", "https://example.test/pricing");
    submit(app.root, "pricing-override");
    await flush();
    const saved = app.saves[0]!;
    assert.equal(saved.longContextThresholdTokens, 1000);
    assert.equal(saved.longContext.freshInputUsdPerMillion, 2);
    assert.equal(saved.shortContext.cacheWrite.length, 4);
    assert.equal(saved.shortContext.cacheWrite.find(value => value.ttl === "oneHour")!.usdPerMillion, 0);
}
finally {
    app.controller.dispose();
} });
test("override removal is explicit and registry strings cannot create active markup", async () => { const app = await mount({ getPricing: async () => ({ prices: [{ ...price(), sourceUrls: ["<script>untrusted</script>"] }], overrides: [price()] }) }); try {
    assert.equal(app.removes.length, 0);
    assert.equal(app.root.querySelector("script"), null);
    assert.equal(app.root.querySelector("a"), null);
    app.root.querySelector<HTMLButtonElement>('[data-remove-price="fixture-model"]')!.click();
    await flush();
    assert.deepEqual(app.removes, ["fixture-model"]);
}
finally {
    app.controller.dispose();
} });
test("operation errors are visible, calculation single-flight and disposal ignores late result", async () => { let resolve!: (result: MoneyEstimate) => void; const pending = new Promise<MoneyEstimate>(yes => { resolve = yes; }); let count = 0; const app = await mount({ estimateCost: async () => { count++; return pending; } }); input(app.root, "pricing-model", "fixture-model"); submit(app.root, "pricing-calculator"); submit(app.root, "pricing-calculator"); assert.equal(count, 1); app.controller.dispose(); const text = app.root.textContent; resolve(estimate()); await flush(); assert.equal(app.root.textContent, text); const failed = await mount({ setPriceOverride: async () => { throw new Error("SECRET"); } }); try {
    input(failed.root, "pricing-override-model", "custom");
    input(failed.root, "pricing-override-mode", "flat");
    submit(failed.root, "pricing-override");
    await flush();
    assert.match(failed.root.textContent ?? "", /override could not be saved/);
    assert.doesNotMatch(failed.root.textContent ?? "", /SECRET/);
}
finally {
    failed.controller.dispose();
} });
test("preview has exact supplied prices with no unknown-model fallback and supports memory overrides", async () => { const bridge = previewPricing([price()]); const tokens: BillingTokens = { freshInput: 1000000, cacheRead: 0, cacheWrite: 0, output: 0, reasoning: 0, outputIncludesReasoning: true }; const context: PricingContext = { inputContext: null, cacheWriteTtl: null }; assert.equal((await bridge.estimateCost!("fixture-model", tokens, context)).usd, 2); assert.equal((await bridge.estimateCost!("unknown", tokens, context)).unavailable, "unknownModel"); await bridge.setPriceOverride!({ ...price(), modelId: "custom" }); assert.equal((await bridge.getPricing!()).overrides.length, 1); await bridge.removePriceOverride!("custom"); assert.equal((await bridge.getPricing!()).overrides.length, 0); });
test("editing a calculator input removes the prior estimate before a new calculation", async () => { const app = await mount({ estimateCost: async () => ({ ...estimate(), usd: 1, unavailable: null }) }); try {
    input(app.root, "pricing-model", "fixture-model");
    submit(app.root, "pricing-calculator");
    await flush();
    assert.ok(app.root.querySelector('#pricing-result'));
    input(app.root, "pricing-count-output", "2");
    assert.equal(app.root.querySelector('#pricing-result'), null);
}
finally {
    app.controller.dispose();
} });
test("reasoning cannot be counted twice or exceed included output", async () => { const app = await mount(); try {
    input(app.root, "pricing-model", "fixture-model");
    input(app.root, "pricing-count-output", "2");
    input(app.root, "pricing-count-reasoning", "3");
    input(app.root, "pricing-reasoning-mode", "yes");
    submit(app.root, "pricing-calculator");
    assert.equal(app.calls.length, 0);
    assert.match(app.root.textContent ?? "", /Reasoning tokens cannot exceed output/);
}
finally {
    app.controller.dispose();
} });
test("preview missing context, TTL, accounting and rates remain separate unavailable reasons", async () => { const tiered = { ...price(), longContextThresholdTokens: 1000 }; const bridge = previewPricing([tiered]); const tokens: BillingTokens = { freshInput: 1, cacheRead: 0, cacheWrite: 1, output: 0, reasoning: null, outputIncludesReasoning: true }; assert.equal((await bridge.estimateCost!(tiered.modelId, tokens, { inputContext: null, cacheWriteTtl: null })).unavailable, 'missingContext'); assert.equal((await bridge.estimateCost!(tiered.modelId, { ...tokens, outputIncludesReasoning: null }, { inputContext: 'short', cacheWriteTtl: null })).unavailable, 'missingAccounting'); assert.equal((await bridge.estimateCost!(tiered.modelId, tokens, { inputContext: 'short', cacheWriteTtl: null })).unavailable, 'missingCacheTtl'); assert.equal((await bridge.estimateCost!(tiered.modelId, tokens, { inputContext: 'short', cacheWriteTtl: 'oneHour' })).unavailable, 'missingPrice'); });
test("mounted dashboard preview uses the single bundled registry and a manual calculation sends no provider request",async()=>{
    const {document}=parseHTML("<html><body><main id='root'></main></body></html>");Object.defineProperty(globalThis,'document',{value:document,configurable:true});const root=document.querySelector('#root') as unknown as HTMLElement;const bridge=previewBridge();const controller=await mountDashboard(root,bridge);
    try{assert.deepEqual((await bridge.getPricing!()).prices,bundledRegistry.models);input(root,'pricing-model',bundledRegistry.models[0]!.modelId);for(const [name,value] of [['freshInput','1000000'],['cacheRead','0'],['cacheWrite','0'],['output','0'],['reasoning','0']])input(root,`pricing-count-${name}`,value!);input(root,'pricing-reasoning-mode','yes');input(root,'pricing-context','short');submit(root,'pricing-calculator');await flush();assert.match(root.querySelector('#pricing-result')!.textContent ?? '',/API-equivalent estimate: USD/);assert.match(root.querySelector('.pricing-section')!.textContent ?? '',/Synthetic preview only/);}
    finally{controller.dispose();}
});
