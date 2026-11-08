import type { MoneyEstimate, PricingView } from "./pricing-types.ts";
import { pricingRateFields, pricingTokenFields, pricingTtls } from "./pricing-inputs.ts";
export interface PricingUiState {
    available: boolean;
    preview: boolean;
    view: PricingView | null;
    estimate: MoneyEstimate | null;
    fields: Record<string, string>;
    busy: boolean;
    error: string | null;
    message: string | null;
    canCalculate: boolean;
    canSave: boolean;
    canRemove: boolean;
}
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] { const result = document.createElement(tag); result.textContent = text; if (className)
    result.className = className; return result; }
function field(state: PricingUiState, id: string, label: string, numeric = false): HTMLElement { const wrapper = node('label', label, 'history-field'); const input = node('input'); input.id = id; input.type = 'text'; if (numeric)
    input.inputMode = 'decimal'; input.value = state.fields[id]!; input.disabled = state.busy; wrapper.append(input); return wrapper; }
function select(state: PricingUiState, id: string, label: string, choices: readonly (readonly [
    string,
    string
])[]): HTMLElement { const wrapper = node('label', label, 'history-field'); const input = node('select'); input.id = id; input.disabled = state.busy; for (const [value, text] of choices) {
    const option = node('option', text);
    option.value = value;
    option.selected = state.fields[id] === value;
    input.append(option);
} wrapper.append(input); return wrapper; }
function button(text: string, id: string, disabled: boolean): HTMLButtonElement { const result = node('button', text, 'wake-button'); result.id = id; result.type = 'submit'; result.disabled = disabled; return result; }
function usd(value: number | null): string { return value === null ? 'Unavailable' : `USD ${value === 0 ? '0' : value.toPrecision(8)}`; }
export function renderPricing(state: PricingUiState): HTMLElement {
    const section = node('section', '', 'pricing-section');
    section.setAttribute('aria-labelledby', 'pricing-title');
    const title = node('h2', 'API-equivalent estimate · USD');
    title.id = 'pricing-title';
    section.append(title, node('p', 'A manual reference calculator, not your actual subscription bill. Enter disjoint token categories. Blank means unknown; enter 0 only for a known zero. Tools, taxes and negotiated discounts are excluded.', 'quota-meta'));
    if (state.preview)
        section.append(node('p', 'Synthetic preview only. Estimates use the explicitly supplied preview registry and memory overrides. No pricing data is fetched.', 'quota-meta'));
    if (!state.available) {
        section.append(node('p', 'Pricing is unavailable in this connection.', 'quota-meta'));
        return section;
    }
    const reload = button('Reload prices', 'pricing-reload', state.busy);
    reload.type = 'button';
    section.append(reload);
    if (!state.canCalculate)
        section.append(node('p', 'Cost calculation is unavailable in this connection.', 'quota-meta'));
    if (!state.canSave)
        section.append(node('p', 'Saving manual overrides is unavailable in this connection.', 'quota-meta'));
    if (!state.canRemove)
        section.append(node('p', 'Removing manual overrides is unavailable in this connection.', 'quota-meta'));
    if (state.view) {
        const registry = node('details');
        registry.append(node('summary', `Reference model prices (${state.view.prices.length})`));
        if (!state.view.prices.length)
            registry.append(node('p', 'No model prices supplied. Exact unknown models remain unavailable.', 'quota-meta'));
        for (const price of state.view.prices) {
            const item = node('article', '', 'quota-card');
            item.append(node('h3', price.modelId), node('p', `${state.view.overrides.some(value => value.modelId === price.modelId) ? 'Manual override' : 'Bundled reference'} · ${price.retrievedOn} · revision ${price.revision}`, 'quota-meta'), node('p', price.longContextThresholdTokens === null ? 'Explicit flat rates across context sizes.' : `Long-context threshold: ${price.longContextThresholdTokens} tokens.`, 'quota-meta'));
            for (const [name, rates] of [['Short context', price.shortContext], ['Long context', price.longContext]] as const)
                item.append(node('p', `${name} · USD per million tokens: fresh input ${rates.freshInputUsdPerMillion === null ? 'Unknown' : rates.freshInputUsdPerMillion}; cache read ${rates.cacheReadUsdPerMillion === null ? 'Unknown' : rates.cacheReadUsdPerMillion}; output ${rates.outputUsdPerMillion === null ? 'Unknown' : rates.outputUsdPerMillion}; cache write ${rates.cacheWrite.map(write => `${pricingTtls.find(([ttl]) => ttl === write.ttl)![1]}: ${write.usdPerMillion === null ? 'Unknown' : write.usdPerMillion}`).join(', ')}.`, 'quota-meta'));
            for (const url of price.sourceUrls)
                item.append(node('p', url, 'quota-meta'));
            if (state.view.overrides.some(value => value.modelId === price.modelId)) {
                const remove = button('Remove this override', '', state.busy || !state.canRemove);
                remove.type = 'button';
                remove.dataset.removePrice = price.modelId;
                item.append(remove);
            }
            registry.append(item);
        }
        section.append(registry);
    }
    const calculator = node('form');
    calculator.id = 'pricing-calculator';
    const inputs = node('div', '', 'pricing-fields');
    inputs.append(field(state, 'pricing-model', 'Exact model ID'));
    for (const [key, label] of pricingTokenFields)
        inputs.append(field(state, `pricing-count-${key}`, label, true));
    inputs.append(select(state, 'pricing-reasoning-mode', 'Output includes reasoning?', [['', 'Unknown'], ['yes', 'Yes · already included in output'], ['no', 'No · reasoning is separate']]), select(state, 'pricing-context', 'Input context rate tier', [['', 'Unknown'], ['short', 'Short'], ['long', 'Long']]), select(state, 'pricing-ttl', 'Cache-write lifetime', [['', 'Unknown'], ...pricingTtls]));
    calculator.append(node('h3', 'Manual token calculator'), inputs, button(state.busy ? 'Working…' : 'Calculate estimate', 'pricing-calculate', state.busy || !state.canCalculate));
    section.append(calculator);
    if (state.estimate) {
        const result = state.estimate;
        const reasons: Record<string, string> = { unknownModel: 'No exact model price is registered.', missingTokens: 'Token counts are missing. Blank counts remain unknown.', missingContext: 'The context tier is unknown for a model with tiered rates.', missingCacheTtl: 'Cache-write lifetime is unknown.', missingPrice: 'A required price is missing.', missingAccounting: 'Whether output already includes reasoning is unknown.' };
        const output = node('div', '', 'quota-card');
        output.id = 'pricing-result';
        output.setAttribute('role', 'status');
        output.setAttribute('aria-live', 'polite');
        output.append(node('h3', result.usd === null ? 'Estimate unavailable' : `API-equivalent estimate: ${usd(result.usd)}`));
        if (result.usd === null)
            output.append(node('p', result.unavailable === null ? 'The service returned no amount or reason.' : Object.hasOwn(reasons, result.unavailable) ? reasons[result.unavailable]! : `Unrecognized estimate status: ${result.unavailable}`, 'quota-meta'));
        output.append(node('p', `Exact model: ${result.modelId} · ${result.manualOverride ? 'Manual override' : 'Reference registry'} · price revision ${result.priceRevision === null ? 'Unavailable' : result.priceRevision} · retrieved ${result.retrievedOn === null ? 'Unavailable' : result.retrievedOn}`, 'quota-meta'));
        const classLabels: Record<string, string> = { freshInput: 'Fresh input', cacheRead: 'Cache read', cacheWrite: 'Cache write', output: 'Output', reasoning: 'Reasoning' };
        for (const part of result.components)
            output.append(node('p', `${Object.hasOwn(classLabels, part.class) ? classLabels[part.class] : `Unrecognized token class: ${part.class}`}: ${part.tokens === null ? 'Unknown' : part.tokens} tokens · ${part.usdPerMillion === null ? 'Unknown rate' : `${part.usdPerMillion} USD per million`} · ${usd(part.usd)}`, 'quota-meta'));
        for (const url of result.sourceUrls)
            output.append(node('p', url, 'quota-meta'));
        section.append(output);
    }
    const editor = node('details');
    editor.append(node('summary', 'Add or replace a manual model price'));
    const form = node('form');
    form.id = 'pricing-override';
    const basics = node('div', '', 'pricing-fields');
    basics.append(field(state, 'pricing-override-model', 'Exact model ID'), select(state, 'pricing-override-mode', 'Context pricing', [['', 'Choose flat or tiered rates'], ['flat', 'Flat rates · same for all context sizes'], ['tiered', 'Separate short and long context rates']]), field(state, 'pricing-context-threshold', 'Long-context threshold tokens (tiered only)', true));
    form.append(node('p', 'Rates are USD per million tokens. Blank rates stay unknown. Saving replaces the complete manual price for this exact model. Manual prices receive the current UTC retrieval date.', 'quota-meta'), basics);
    for (const context of ['short', 'long']) {
        const group = node('fieldset');
        group.hidden = context === 'long' && state.fields['pricing-override-mode'] !== 'tiered';
        group.append(node('legend', context === 'short' ? 'Short / flat context rates' : 'Long context rates'));
        const fields = node('div', '', 'pricing-fields');
        for (const [key, label] of pricingRateFields)
            fields.append(field(state, `pricing-${context}-${key}`, `${label} USD / million`, true));
        for (const [ttl, label] of pricingTtls)
            fields.append(field(state, `pricing-${context}-write-${ttl}`, `Cache write · ${label} USD / million`, true));
        group.append(fields);
        form.append(group);
    }
    const sourceLabel = node('label', 'Source URLs (optional, HTTPS, one per line)', 'history-field');
    const sources = node('textarea');
    sources.id = 'pricing-sources';
    sources.value = state.fields['pricing-sources']!;
    sources.disabled = state.busy;
    sourceLabel.append(sources);
    form.append(sourceLabel, button('Save manual override', 'pricing-save', state.busy || !state.canSave));
    editor.append(form);
    section.append(editor);
    for (const [text, className] of [[state.error, 'error-notice'], [state.message, 'quota-meta']])
        if (text) {
            const message = node('p', text, className!);
            message.setAttribute('role', 'status');
            section.append(message);
        }
    return section;
}
