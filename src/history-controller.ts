import type { Bridge } from "./types.ts";
import type { HistoryRange } from "./history-types.ts";
import { renderClearHistoryDialog, renderHistory } from "./history-ui.ts";
import type { HistoryUiState } from "./history-ui.ts";
export function createHistoryController(root: HTMLElement, content: HTMLElement, bridge: Bridge,onMaintenanceChanged?:()=>void) {
    const element = document.createElement("div");
    const state: HistoryUiState = { available: !!bridge.getHistory, range: 24, data: null, loading: false, error: null, retention: null, draftRetention: null, settingsError: null, saving: false, clearing: false, message: null, visibleRows: 100 };
    let disposed = false;
    let revision = 0;
    let dialog: HTMLElement | null = null;
    function paint() {
        if (disposed)
            return;
        const focusId = element.contains(document.activeElement) ? document.activeElement?.id : undefined;
        const expanded = Array.from(element.querySelectorAll("details")).map(detail => detail.open);
        element.replaceChildren(renderHistory(state));
        element.querySelectorAll("details").forEach((detail, index) => { detail.open = expanded[index] === true; });
        element.querySelector<HTMLSelectElement>("#history-range")?.addEventListener("change", event => { const value = Number((event.currentTarget as HTMLSelectElement).value); if (value === 24 || value === 168 || value === 720) {
            state.range = value;
            state.visibleRows = 100;
            state.message = null;
            void load();
        } });
        element.querySelector("#history-reload")?.addEventListener("click", () => { void load(); });
        element.querySelector<HTMLSelectElement>("#history-retention")?.addEventListener("change", event => { state.draftRetention = Number((event.currentTarget as HTMLSelectElement).value); });
        element.querySelector("#history-save")?.addEventListener("click", () => { void saveRetention(); });
        element.querySelector("#history-clear")?.addEventListener("click", openClear);
        for (const id of ["history-more", "history-more-checks"])
            element.querySelector(`#${id}`)?.addEventListener("click", () => { state.visibleRows += 100; paint(); });
        if (focusId)
            element.querySelector<HTMLElement>(`#${focusId}`)?.focus();
    }
    async function load() {
        if (disposed || !bridge.getHistory)
            return;
        const current = ++revision;
        const range = state.range;
        state.loading = true;
        state.error = null;
        paint();
        try {
            const result = await bridge.getHistory(range);
            if (disposed || current !== revision)
                return;
            if (result.rangeHours !== range) {
                state.data = null;
                state.error = "History returned a different time range. Reload history to try again.";
            }
            else {
                state.data = result;
                if(state.draftRetention===state.retention)state.draftRetention=result.retentionDays;
                state.retention=result.retentionDays;
            }
        }
        catch {
            if (!disposed && current === revision) {
                state.data = null;
                state.error = "History could not be loaded. Reload history to try again.";
            }
        }
        finally {
            if (!disposed && current === revision) {
                state.loading = false;
                paint();
            }
        }
    }
    async function saveRetention() {
        if (disposed || state.saving || state.clearing || state.draftRetention === null)
            return;
        if (!bridge.setHistoryRetention) {
            state.settingsError = "Retention cannot be changed in this connection.";
            paint();
            return;
        }
        state.saving = true;
        state.settingsError = null;
        state.message = null;
        paint();
        try {
            const result = await bridge.setHistoryRetention(state.draftRetention);
            if (disposed)
                return;
            state.retention = result.retentionDays;
            state.draftRetention = result.retentionDays;
            state.message = "Retention saved. Older history has been removed according to this setting.";
            await load();
        }
        catch {
            if (!disposed)
                state.settingsError = "Retention could not be saved. The saved setting has not been changed here.";
        }
        finally {
            state.saving = false;
            onMaintenanceChanged?.();
            paint();
        }
    }
    function close() { dialog?.remove(); dialog = null; content.removeAttribute("inert"); if (!disposed)
        element.querySelector<HTMLElement>("#history-clear")?.focus(); }
    function openClear() {
        if (disposed || dialog || state.clearing || state.saving || root.querySelector('[role="dialog"]'))
            return;
        if (!bridge.clearHistory) {
            state.settingsError = "History cannot be cleared in this connection.";
            paint();
            return;
        }
        dialog = renderClearHistoryDialog(() => { void clear(); }, () => { close(); });
        root.append(dialog);
        content.setAttribute("inert", "");
        dialog.querySelector<HTMLElement>("#history-cancel")?.focus();
    }
    async function clear() {
        if (disposed || !dialog || state.clearing || !bridge.clearHistory)
            return;
        close();
        state.clearing = true;
        state.message = null;
        state.settingsError = null;
        ++revision;
        state.loading = false;
        paint();
        try {
            await bridge.clearHistory(true);
            if (disposed)
                return;
            state.data = null;
            state.message = "History cleared. Future checks record history again.";
            await load();
        }
        catch {
            if (!disposed)
                state.settingsError = "History could not be cleared. Reload history to check what is saved.";
        }
        finally {
            state.clearing = false;
            onMaintenanceChanged?.();
            paint();
            if (!disposed)
                element.querySelector<HTMLElement>("#history-clear")?.focus();
        }
    }
    paint();
    return { element, reload: load, async start() {
            const settings = async () => {
                if (!bridge.getHistorySettings) {
                    state.settingsError = "History retention settings are unavailable in this connection.";
                    paint();
                    return;
                }
                try {
                    const result = await bridge.getHistorySettings();
                    if (disposed)
                        return;
                    state.retention = result.retentionDays;
                    state.draftRetention = result.retentionDays;
                }
                catch {
                    if (!disposed)
                        state.settingsError = "History retention settings could not be loaded.";
                }
                paint();
            };
            await Promise.all([load(), settings()]);
        }, dispose() { disposed = true; ++revision; dialog?.remove(); dialog = null; content.removeAttribute("inert"); } };
}
