import type { HistoryRange, HistoryView } from "./history-types.ts";
import { renderHistoryChart } from "./history-chart.ts";
export interface HistoryUiState {
    available: boolean;
    range: HistoryRange;
    data: HistoryView | null;
    loading: boolean;
    error: string | null;
    retention: number | null;
    draftRetention: number | null;
    settingsError: string | null;
    saving: boolean;
    clearing: boolean;
    message: string | null;
    visibleRows: number;
}
export function historyNode<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] { const node = document.createElement(tag); node.textContent = text; if (className)
    node.className = className; return node; }
function select(id: string, label: string, choices: readonly (readonly [
    number,
    string
])[], value: number | null): HTMLElement {
    const wrapper = historyNode("label", label, "history-field");
    const control = historyNode("select");
    control.id = id;
    if (value === null) {
        const option = historyNode("option", "Unavailable");
        option.value = "";
        option.selected = true;
        control.append(option);
        control.disabled = true;
    }
    for (const [key, text] of choices) {
        const option = historyNode("option", text);
        option.value = String(key);
        option.selected = key === value;
        control.append(option);
    }
    wrapper.append(control);
    return wrapper;
}
export function renderHistory(state: HistoryUiState): HTMLElement {
    const section = historyNode("section", "", "history-section");
    section.setAttribute("aria-labelledby", "history-title");
    const heading = historyNode("h2", "Usage history");
    heading.id = "history-title";
    section.append(heading);
    if (!state.available) {
        section.append(historyNode("p", "History is unavailable in this connection.", "quota-meta"));
        return section;
    }
    const controls = historyNode("div", "", "history-controls");
    controls.append(select("history-range", "Time range", [[24, "24 hours"], [168, "7 days"], [720, "30 days"]], state.range));
    const reload = historyNode("button", "Reload history", "wake-button");
    reload.id = "history-reload";
    reload.type = "button";
    reload.disabled = state.loading || state.clearing;
    controls.append(reload);
    section.append(controls);
    const status = historyNode("p", state.loading ? "Loading saved history…" : state.error ?? state.message ?? (state.data ? "Saved observations on this device." : "History has not loaded yet."), state.error ? "error-notice" : "quota-meta");
    status.setAttribute("role", "status");
    status.setAttribute("aria-live", "polite");
    section.append(status);
    const history = state.data;
    if (history && !state.loading && !state.error) {
        if (history.error)
            section.append(historyNode("p", `History is unavailable (${history.error}). Existing data has been preserved.`, "error-notice"));
        if (history.truncated)
            section.append(historyNode("p", "Result limit reached. This range is incomplete; select a shorter range to see more detail. Each query returns at most 10,000 samples and 10,000 checks.", "error-notice"));
        const bin = state.range === 24 ? "5-minute" : state.range === 168 ? "hourly" : "6-hour";
        section.append(historyNode("p", `Charts show the last reported sample per ${bin} bin and reset window, at its original observation time. Gaps, unknown values, collection failures and changed reset windows are not connected. Percentages are provider-reported usage, not tokens or cost.`, "quota-meta"));
        if (!history.error && !history.samples.length)
            section.append(historyNode("p", history.checks.length ? "No reported quota samples in this range. Collection checks are listed below." : "No history yet in this range. Future checks record history while Jentrics is running.", "history-empty"));
        const groups = new Map<string, typeof history.samples>();
        for (const sample of history.samples) {
            const key = `${sample.agentId} · ${sample.bucketId}`;
            const group = groups.get(key);
            if (group)
                group.push(sample);
            else
                groups.set(key, [sample]);
        }
        for (const [label, samples] of groups) {
            const figure = historyNode("figure", "", "history-figure");
            figure.append(historyNode("figcaption", label), renderHistoryChart(history, samples, label));
            section.append(figure);
        }
        if (history.samples.length) {
            const detail = historyNode("details");
            detail.append(historyNode("summary", `Reported observations (${history.samples.length})`));
            const scroll = historyNode("div", "", "history-table-wrap");
            const table = historyNode("table", "", "history-table");
            table.append(historyNode("caption", "Saved observations · original times and provider sources"));
            const head = historyNode("thead");
            const row = historyNode("tr");
            for (const name of ["Provider / bucket", "Observed", "Used", "Window", "Reset", "Source"]) {
                const th = historyNode("th", name);
                th.scope = "col";
                row.append(th);
            }
            head.append(row);
            table.append(head);
            const body = historyNode("tbody");
            for (const sample of [...history.samples].sort((a, b) => b.observedMs - a.observedMs).slice(0, state.visibleRows)) {
                const row = historyNode("tr");
                for (const text of [`${sample.agentId} / ${sample.bucketId}`, new Date(sample.observedMs).toLocaleString(), sample.usedPercent === null ? "Unknown" : `${sample.usedPercent}%`, sample.windowMinutes === null ? "Unknown" : `${sample.windowMinutes} min`, sample.resetsAtMs === null ? "Unknown" : new Date(sample.resetsAtMs).toLocaleString(), sample.source])
                    row.append(historyNode("td", text));
                body.append(row);
            }
            table.append(body);
            scroll.append(table);
            detail.append(scroll);
            if (history.samples.length > state.visibleRows) {
                const more = historyNode("button", `Show more observations (${state.visibleRows} of ${history.samples.length} shown)`, "wake-button");
                more.id = "history-more";
                more.type = "button";
                detail.append(more);
            }
            section.append(detail);
        }
        if (history.checks.length) {
            const detail = historyNode("details");
            detail.append(historyNode("summary", `Collection checks (${history.checks.length})`));
            const list = historyNode("ul", "", "history-checks");
            for (const check of [...history.checks].sort((a, b) => b.observedMs - a.observedMs).slice(0, state.visibleRows))
                list.append(historyNode("li", `${check.agentId} · ${new Date(check.observedMs).toLocaleString()} · ${check.status}${check.diagnosticCode ? ` (${check.diagnosticCode})` : ""}`));
            detail.append(list);
            if (history.checks.length > state.visibleRows) {
                const more = historyNode("button", `Show more checks (${state.visibleRows} of ${history.checks.length} shown)`, "wake-button");
                more.id = "history-more-checks";
                more.type = "button";
                detail.append(more);
            }
            section.append(detail);
        }
    }
    const settings = historyNode("div", "", "history-settings");
    settings.append(select("history-retention", "Keep history", [[7, "7 days"], [30, "30 days"], [90, "90 days"], [180, "180 days"], [365, "365 days"]], state.draftRetention));
    const save = historyNode("button", state.saving ? "Saving…" : "Save retention", "wake-button");
    save.id = "history-save";
    save.type = "button";
    save.disabled = state.retention === null || state.saving || state.clearing;
    const clear = historyNode("button", state.clearing ? "Clearing…" : "Clear history…", "wake-button");
    clear.id = "history-clear";
    clear.type = "button";
    clear.disabled = state.clearing || state.saving;
    settings.append(save, clear);
    section.append(settings, historyNode("p", "Retention and clearing affect saved history only. Latest provider snapshots remain. Future checks record history again.", "quota-meta"));
    if (state.settingsError)
        section.append(historyNode("p", state.settingsError, "error-notice"));
    return section;
}
export function renderClearHistoryDialog(confirm: () => void, cancel: () => void): HTMLElement {
    const overlay = historyNode("div", "", "wake-overlay");
    const dialog = historyNode("section", "", "wake-dialog");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-labelledby", "history-clear-title");
    dialog.setAttribute("aria-describedby", "history-clear-description");
    const title = historyNode("h2", "Clear saved history?");
    title.id = "history-clear-title";
    const description = historyNode("p", "This deletes saved usage observations and collection checks on this device. Latest provider snapshots remain. Future checks record history again.");
    description.id = "history-clear-description";
    const actions = historyNode("div", "", "wake-dialog-actions");
    const no = historyNode("button", "Cancel", "wake-button");
    no.id = "history-cancel";
    no.type = "button";
    no.addEventListener("click", cancel);
    const yes = historyNode("button", "Clear history", "refresh-button");
    yes.id = "history-confirm";
    yes.type = "button";
    yes.addEventListener("click", confirm);
    actions.append(no, yes);
    dialog.append(title, description, actions);
    overlay.append(dialog);
    overlay.addEventListener("keydown", event => { if (event.key === "Escape") {
        event.preventDefault();
        cancel();
    } if (event.key === "Tab") {
        event.preventDefault();
        (document.activeElement === no ? yes : no).focus();
    } });
    return overlay;
}
