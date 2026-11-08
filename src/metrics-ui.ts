import {telemetrySetupGuide} from "./telemetry-guide.ts";
import type { MetricRange, MetricSettings, MetricView } from "./metrics-types.ts";
import {metricTrendChoices,renderMetricsTrend} from "./metrics-trend.ts";
import type {MetricTrendKind} from "./metrics-trend.ts";
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = ""): HTMLElementTagNameMap[K] { const element = document.createElement(tag); element.textContent = text; return element; }
function select(id: string, label: string, values: number[], selected: number): HTMLElement {
    const wrapper = node("label", label), input = node("select");
    input.id = id;
    for (const value of values) {
        const option = node("option", `${value} days`);
        option.value = String(value);
        option.selected = value === selected;
        input.append(option);
    }
    wrapper.append(input);
    return wrapper;
}
export interface MetricRenderOptions {
    rangeDays?: MetricRange;
    draft?: MetricSettings | null;
    loading?: boolean;
    message?: string | null;
    trend?:MetricTrendKind;
}
function renderSettings(settings: MetricSettings): HTMLElement {
    const form = node("form");
    form.id = "metrics-settings";
    const enabled = node("label", "Enable local telemetry collector"), checkbox = node("input");
    checkbox.id = "metrics-enabled";
    checkbox.type = "checkbox";
    checkbox.checked = settings.enabled;
    enabled.prepend(checkbox);
    form.append(enabled, select("metrics-event-retention", "Keep individual exports", [7, 30, 90], settings.eventRetentionDays), select("metrics-daily-retention", "Keep daily aggregates", [30, 90, 365], settings.dailyRetentionDays));
    const save = node("button", "Save telemetry settings");
    save.id = "metrics-save";
    save.type = "submit";
    save.className = "wake-button";
    form.append(save);
    return form;
}
export function renderMetrics(view: MetricView | null, error: string | null, preview: boolean, options: MetricRenderOptions = {}): HTMLElement {
    const section = node("section");
    section.className = "history-section";
    section.append(node("h2", "Local telemetry"), node("p", "Optional numeric-only collection on this computer. Account totals and quota percentages are separate. Collection starts only after you enable it and configure a supported provider to export metrics."));
    if (preview)
        section.append(node("p", "Synthetic preview only. This switch does not open a port or change provider settings."));
    if (error) {
        const message = node("p", error);
        message.setAttribute("role", "status");
        section.append(message);
    }
    const reload = node("button", "Reload telemetry");
    reload.id = "metrics-reload";
    reload.className = "wake-button";
    section.append(reload);
    const range = select("metrics-range", "Telemetry history range", [1, 7, 30, 365], options.rangeDays ?? 30);
    range.className = "history-field";
    section.append(range);
    const trendLabel=node("label","Measurement");trendLabel.className="history-field";const trend=node("select");trend.id="metrics-trend";for(const [value,text] of metricTrendChoices){const option=node("option",text);option.value=value;option.selected=value===(options.trend ?? "tokens");trend.append(option);}trendLabel.append(trend);section.append(trendLabel);
    if (options.loading) {
        const loading = node("p", "Loading telemetry for the selected range…");
        loading.setAttribute("role", "status");
        section.append(loading);
    }
    if (options.message) {
        const message = node("p", options.message);
        message.setAttribute("role", "status");
        section.append(message);
    }
    const settings = options.draft ?? view?.settings;
    if (!view) {
        if (settings)
            section.append(renderSettings(settings));
        section.append(node("p", "Telemetry settings and observations are unavailable."));
        return section;
    }
    section.append(node("p", `${view.running ? "Collector running" : view.settings.enabled ? "Collector unavailable" : "Collector disabled"} · ${view.endpoint}`));
    if (view.status.error) {
        const error = node("p", view.status.error);
        error.className = "error-notice";
        section.append(error);
    }
    section.append(node("p", view.status.coverageStartedMs === null ? "No collection coverage in this app run." : `Current coverage began ${new Date(view.status.coverageStartedMs).toLocaleString()}. Cumulative counters start with a baseline; missing periods and earlier app runs are not inferred.`), node("p", `Baseline points: ${view.status.baselinePoints} · Duplicate points: ${view.status.duplicatePoints} · Ignored metrics: ${view.status.ignoredMetrics} · Rejected exports / transport incidents: ${view.status.rejectedBatches}`));
    section.append(renderSettings(settings!));
    const setup = node("details");
    setup.append(node("summary", "Provider setup and coverage"), node("p", "Jentrics accepts only OTLP HTTP/JSON metrics at the loopback endpoint above. Logs, traces, protobuf, compression, unknown counter shapes and named agent/tool attribution are rejected. Prompts, tool arguments, repository paths and transcripts are discarded before storage. No provider configuration is changed by this switch."), node("p", "Use the telemetry setup guide for Claude Code or Gemini CLI. Disable prompt logging and log/trace exports; enable metrics only. Unsupported effective configurations remain unavailable."));
    const guideDetails=node("details");
    const guide=node("pre",telemetrySetupGuide);guide.className="wake-prompt";
    guideDetails.append(node("summary","Read bundled telemetry setup guide"),guide);
    setup.append(guideDetails);
    section.append(setup);
    section.append(node("p", "Median, p95, TTFT and output tokens/second: unavailable for these metric sources. Claude active time measures activity, not request latency. Gemini latency sum/count supports a mean only; no percentile is inferred."));
    const means = view.history.daily.filter(row => row.metric === "requestLatencyMs" && row.count !== null && row.count > 0);
    if (means.length) {
        const sum = means.reduce((value, row) => value + row.value, 0), count = means.reduce((value, row) => value + row.count!, 0);
        section.append(node("p", `Observed Gemini mean request latency: ${(sum / count).toFixed(1)} ms (${count} requests in retained daily exports).`));
    }
    if (view.history.truncated)
        section.append(node("p", "This result reached the 10,000-row limit and is incomplete."));
    if (!view.history.daily.length)
        section.append(node("p", "No local metrics have been captured in this range. Unknown token classes remain unavailable."));
    section.append(renderMetricsTrend(view,options.trend ?? "tokens"));
    const clear = node("button", "Clear local telemetry…");
    clear.id = "metrics-clear";
    clear.className = "wake-button";
    section.append(clear);
    return section;
}
export function renderMetricClear(confirm: () => void, cancel: () => void): HTMLElement {
    const overlay = node("div");
    overlay.className = "wake-overlay";
    const dialog = node("section");
    dialog.className = "wake-dialog";
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-label", "Clear local telemetry?");
    dialog.append(node("h2", "Clear local telemetry?"), node("p", "Delete numeric export history and daily aggregates on this computer. Quota history, provider settings and reference prices remain. Future exports record data again."));
    const no = node("button", "Cancel");
    no.id = "metrics-cancel";
    no.addEventListener("click", cancel);
    const yes = node("button", "Clear telemetry");
    yes.id = "metrics-confirm";
    yes.addEventListener("click", confirm);
    dialog.append(no, yes);
    dialog.addEventListener("keydown", event => {
        if (event.key === "Escape") {
            event.preventDefault();
            cancel();
        }
        if (event.key === "Tab") {
            event.preventDefault();
            (document.activeElement === no ? yes : no).focus();
        }
    });
    overlay.append(dialog);
    return overlay;
}
