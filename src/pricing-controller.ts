import type { PricingBridge } from "./pricing-types.ts";
import { calculatorInput, initialPricingFields, manualPrice } from "./pricing-inputs.ts";
import { renderPricing } from "./pricing-ui.ts";
import type { PricingUiState } from "./pricing-ui.ts";
export function createPricingController(bridge: PricingBridge) {
    const element = document.createElement('div');
    const state: PricingUiState = { available: !!bridge.getPricing, preview: bridge.preview, view: null, estimate: null, fields: initialPricingFields(), busy: false, error: null, message: null, canCalculate: !!bridge.estimateCost, canSave: !!bridge.setPriceOverride, canRemove: !!bridge.removePriceOverride };
    let disposed = false;
    let revision = 0;
    function paint() { if (disposed)
        return; const focusId = element.contains(document.activeElement) ? document.activeElement?.id : undefined; const expanded = Array.from(element.querySelectorAll('details')).map(detail => detail.open); element.replaceChildren(renderPricing(state)); element.querySelectorAll('details').forEach((detail, index) => { detail.open = expanded[index] === true; }); element.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>('input,select,textarea').forEach(input => { input.addEventListener(input.tagName === 'SELECT' ? 'change' : 'input', () => { state.fields[input.id] = input.value; state.estimate = null; element.querySelector("#pricing-result")?.remove(); if (input.id === 'pricing-override-mode')
        paint(); }); }); element.querySelector('#pricing-reload')?.addEventListener('click', () => { void load(); }); element.querySelector('#pricing-calculator')?.addEventListener('submit', event => { event.preventDefault(); void calculate(); }); element.querySelector('#pricing-override')?.addEventListener('submit', event => { event.preventDefault(); void save(); }); element.querySelectorAll<HTMLButtonElement>('[data-remove-price]').forEach(button => button.addEventListener('click', () => { void remove(button.dataset.removePrice!); })); if (focusId)
        element.querySelector<HTMLElement>(`#${focusId}`)?.focus(); }
    async function load() { if (disposed || state.busy || !bridge.getPricing)
        return; const current = ++revision; state.error = null; state.busy = true; paint(); try {
        const result = await bridge.getPricing();
        if (!disposed && current === revision)
            state.view = result;
    }
    catch {
        if (!disposed && current === revision) {
            state.view = null;
            state.error = 'Model prices could not be loaded. No replacement or fallback price was used.';
        }
    }
    finally {
        if (!disposed && current === revision) {
            state.busy = false;
            paint();
        }
    } }
    async function calculate() { if (disposed || state.busy || !bridge.estimateCost)
        return; let input: ReturnType<typeof calculatorInput>; try {
        input = calculatorInput(state.fields);
    }
    catch (error) {
        state.error = error instanceof Error ? error.message : 'Calculator input is invalid.';
        paint();
        return;
    } state.busy = true; state.error = null; state.message = null; state.estimate = null; const current = ++revision; paint(); try {
        const result = await bridge.estimateCost(input.modelId, input.tokens, input.context);
        if (!disposed && current === revision)
            state.estimate = result;
    }
    catch {
        if (!disposed && current === revision)
            state.error = 'The estimate could not be calculated. No amount is confirmed.';
    }
    finally {
        if (!disposed && current === revision) {
            state.busy = false;
            paint();
        }
    } }
    async function save() { if (disposed || state.busy || !bridge.setPriceOverride)
        return; let price: ReturnType<typeof manualPrice>; try {
        price = manualPrice(state.fields, new Date());
    }
    catch (error) {
        state.error = error instanceof Error ? error.message : 'Manual price input is invalid.';
        paint();
        return;
    } state.busy = true; state.error = null; state.message = null; state.estimate = null; ++revision; paint(); try {
        const result = await bridge.setPriceOverride(price);
        if (disposed)
            return;
        state.view = result;
        state.message = 'Manual price override saved. This is a reference estimate, not your actual subscription bill.';
    }
    catch {
        if (!disposed)
            state.error = 'The manual price override could not be saved. Reload prices to verify the saved configuration.';
    }
    finally {
        state.busy = false;
        paint();
    } }
    async function remove(modelId: string) { if (disposed || state.busy || !bridge.removePriceOverride)
        return; state.busy = true; state.error = null; state.message = null; state.estimate = null; ++revision; paint(); try {
        const result = await bridge.removePriceOverride(modelId);
        if (disposed)
            return;
        state.view = result;
        state.message = 'Manual override removed. Only an exact remaining reference price can be used.';
    }
    catch {
        if (!disposed)
            state.error = 'The manual price override could not be removed. Reload prices to verify the saved configuration.';
    }
    finally {
        state.busy = false;
        paint();
    } }
    paint();
    return { element, start: load, reload:load, dispose() { disposed = true; ++revision; } };
}
