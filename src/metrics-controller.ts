import type { MetricBridge, MetricRange, MetricSettings, MetricView } from "./metrics-types.ts";
import { renderMetrics, renderMetricClear } from "./metrics-ui.ts";
import {metricTrendChoices} from "./metrics-trend.ts";
import type {MetricTrendKind} from "./metrics-trend.ts";
export function createMetricsController(root: HTMLElement, content: HTMLElement, bridge: MetricBridge,onMaintenanceChanged?:()=>void) {
    const element = document.createElement("div");
    let view: MetricView | null = null;
    let draft: MetricSettings | null = null;
    let dirty = false;
    let range: MetricRange = 30;
    let trend:MetricTrendKind="tokens";
    let queryError: string | null = null;
    let operationError: string | null = null;
    let message: string | null = null;
    let loading = false;
    let busy = false;
    let disposed = false;
    let revision = 0;
    let dialog: HTMLElement | null = null;
    function paint() {
        if (disposed)
            return;
        const focus = element.contains(document.activeElement) ? document.activeElement?.id : null;
        const expanded = Array.from(element.querySelectorAll("details")).map(detail => detail.open);
        element.replaceChildren(renderMetrics(view, operationError ?? queryError, bridge.preview, { rangeDays: range, draft, loading, message,trend }));
        element.querySelectorAll("details").forEach((detail, index) => { detail.open = expanded[index] === true; });
        element.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLButtonElement>("input,select,button").forEach(control => { control.disabled = busy; });
        const saveButton = element.querySelector<HTMLButtonElement>("#metrics-save");
        if (saveButton)
            saveButton.disabled = busy || !bridge.setMetricSettings;
        const clearButton = element.querySelector<HTMLButtonElement>("#metrics-clear");
        if (clearButton)
            clearButton.disabled = busy || !bridge.clearMetricHistory;
        element.querySelector("#metrics-reload")?.addEventListener("click", () => { operationError = null; message = null; void reload(); });
        element.querySelector<HTMLSelectElement>("#metrics-trend")?.addEventListener("change",event=>{const selected=(event.currentTarget as HTMLSelectElement).value;const choice=metricTrendChoices.find(([value])=>value===selected);if(choice){trend=choice[0];paint();}});
        element.querySelector<HTMLSelectElement>("#metrics-range")?.addEventListener("change", event => { const next = Number((event.currentTarget as HTMLSelectElement).value); if (next === 1 || next === 7 || next === 30 || next === 365) {
            range = next;
            view = null;
            message = null;
            void reload();
        } });
        element.querySelector<HTMLInputElement>("#metrics-enabled")?.addEventListener("change", event => { if (draft) {
            draft.enabled = (event.currentTarget as HTMLInputElement).checked;
            dirty = true;
        } });
        for (const [id, key] of [["metrics-event-retention", "eventRetentionDays"], ["metrics-daily-retention", "dailyRetentionDays"]] as const)
            element.querySelector<HTMLSelectElement>(`#${id}`)?.addEventListener("change", event => { if (draft) {
                draft[key] = Number((event.currentTarget as HTMLSelectElement).value);
                dirty = true;
            } });
        element.querySelector("#metrics-settings")?.addEventListener("submit", event => { event.preventDefault(); void save(); });
        element.querySelector("#metrics-clear")?.addEventListener("click", open);
        if (focus)
            element.querySelector<HTMLElement>(`#${focus}`)?.focus();
    }
    async function reload(afterMutation=false) {
        if (disposed || (busy && !afterMutation))
            return;
        const current = ++revision;
        const requestedRange = range;
        loading = true;
        queryError = null;
        paint();
        try {
            if (!bridge.getMetrics)
                throw new Error("Telemetry connection unavailable");
            const next = await bridge.getMetrics(requestedRange);
            if (disposed || current !== revision)
                return;
            view = next;
            if (!dirty) {
                draft = { ...next.settings };
            }
        }
        catch {
            if (!disposed && current === revision)
                queryError = "Local telemetry could not be loaded. Reload to verify its state.";
        }
        finally {
            if (!disposed && current === revision) {
                loading = false;
                paint();
            }
        }
    }
    async function save() {
        if (disposed || busy || !draft || !bridge.setMetricSettings)
            return;
        if (![7, 30, 90].includes(draft.eventRetentionDays) || ![30, 90, 365].includes(draft.dailyRetentionDays) || draft.dailyRetentionDays < draft.eventRetentionDays) {
            operationError = "Daily retention must be at least as long as individual-export retention. Choose supported retention values.";
            paint();
            return;
        }
        const settings = { ...draft };
        ++revision;
        loading = false;
        busy = true;
        operationError = null;
        message = null;
        paint();
        try {
            const next = await bridge.setMetricSettings(settings);
            if (disposed)
                return;
            draft = { ...next.settings };
            dirty = false;
            view = range === 30 ? next : null;
            queryError = null;
            message = "Telemetry settings saved. Provider configuration is unchanged.";
            // The settings response contains the service's default 30-day view.
            // Requery other selected ranges rather than relabeling that history.
            if (range !== 30) {
                await reload(true);
            }
        }
        catch {
            if (!disposed)
                operationError = "Telemetry settings could not be saved. Reload to verify the current collector state.";
        }
        finally {
            busy = false;
            onMaintenanceChanged?.();
            paint();
        }
    }
    function close() { dialog?.remove(); dialog = null; content.removeAttribute("inert"); if (!disposed)
        element.querySelector<HTMLElement>("#metrics-clear")?.focus(); }
    function open() { if (busy || disposed || dialog || root.querySelector('[role="dialog"]') || !bridge.clearMetricHistory)
        return; dialog = renderMetricClear(() => { void clear(); }, close); root.append(dialog); content.setAttribute("inert", ""); dialog.querySelector<HTMLElement>("#metrics-cancel")?.focus(); }
    async function clear() {
        if (busy || disposed || !dialog || !bridge.clearMetricHistory)
            return;
        close();
        ++revision;
        loading = false;
        busy = true;
        operationError = null;
        message = null;
        paint();
        try {
            await bridge.clearMetricHistory(true);
            if (disposed)
                return;
            if (view)
                view = { ...view, history: { events: [], daily: [], truncated: false } };
            message = "Local telemetry cleared. Future exports record data again.";
            await reload(true);
        }
        catch {
            if (!disposed)
                operationError = "Telemetry could not be cleared. Reload to verify retained data.";
        }
        finally {
            busy = false;
            onMaintenanceChanged?.();
            paint();
        }
    }
    paint();
    return { element, start: reload, reload, dispose() { disposed = true; ++revision; dialog?.remove(); dialog = null; content.removeAttribute("inert"); } };
}
