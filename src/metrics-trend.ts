import type { DailyMetric, MetricView } from "./metrics-types.ts";
export type MetricTrendKind = "tokens" | "costUsd" | "activeSeconds" | "requestLatencyMs" | "apiRequests";
export const metricTrendChoices: readonly (readonly [
    MetricTrendKind,
    string
])[] = [["tokens", "Local tokens"], ["costUsd", "Local reported cost (USD)"], ["activeSeconds", "Local active time (seconds)"], ["requestLatencyMs", "Mean request latency (ms)"], ["apiRequests", "Provider requests / errors (counts)"]];
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] { const result = document.createElement(tag); result.textContent = text; if (className)
    result.className = className; return result; }
function svg<K extends keyof SVGElementTagNameMap>(tag: K, attributes: Record<string, string | number> = {}, text = ""): SVGElementTagNameMap[K] { const result = document.createElementNS("http://www.w3.org/2000/svg", tag); for (const [key, value] of Object.entries(attributes))
    result.setAttribute(key, String(value)); result.textContent = text; return result; }
function measured(row: DailyMetric, kind: MetricTrendKind): number | null { if (kind !== "requestLatencyMs")
    return row.value; return row.count !== null && row.count > 0 ? row.value / row.count : null; }
function format(value: number): string { return Number.isInteger(value) ? new Intl.NumberFormat().format(value) : value.toPrecision(8); }
export function renderMetricsTrend(view: MetricView, kind: MetricTrendKind): HTMLElement {
    const section = node("section", "", "metrics-trend");
    const title = metricTrendChoices.find(([value]) => value === kind)![1];
    section.append(node("h3", title), node("p", "Source: captured local numeric provider exports, grouped by interval-end day (UTC). Account totals remain separate. Discrete marks show observed days only; missing days and coverage gaps are never filled or connected.", "quota-meta"));
    if (kind === "costUsd")
        section.append(node("p", "Reported local USD amounts are not a subscription invoice and are separate from the reference-price calculator.", "quota-meta"));
    if (kind === "apiRequests")
        section.append(node("p", "Provider success, error and unknown-status counters stay separate. Receiver transport/rejected-export counters are separate; no error rate is inferred.", "quota-meta"));
    const rows = view.history.daily.filter(row => row.metric === kind);
    if (!rows.length) {
        section.append(node("p", "No captured values for this measurement in the selected range. Missing data is unavailable, not zero.", "quota-meta"));
        return section;
    }
    const groups = new Map<string, DailyMetric[]>();
    for (const row of rows) {
        const key = JSON.stringify([row.provider, row.model, row.dimension]);
        const group = groups.get(key);
        if (group)
            group.push(row);
        else
            groups.set(key, [row]);
    }
    for (const group of groups.values()) {
        const sample = group[0]!;
        const label = `${sample.provider} · ${sample.model === null ? "Model not reported" : sample.model} · ${sample.dimension === null ? "Class not reported" : sample.dimension}`;
        const figure = node("figure", "", "history-figure");
        figure.append(node("figcaption", label));
        const known = group.filter(row => measured(row, kind) !== null);
        if (!known.length) {
            figure.append(node("p", "Mean unavailable: no positive reported request count supports a measured mean.", "quota-meta"));
            section.append(figure);
            continue;
        }
        const minimum = Math.min(...group.map(row => row.dayMs));
        const maximum = Math.max(...group.map(row => row.dayMs));
        const days = (maximum - minimum) / 86400000 + 1;
        const scale = Math.max(...known.map(row => measured(row, kind)!));
        const chart = svg("svg", { viewBox: "0 0 640 180", role: "img", "aria-label": `${title}: ${label}. Discrete reported daily values; original numeric source rows follow in the table.`, class: "history-chart" });
        chart.append(svg("title", {}, `${title} · ${label}`));
        chart.append(svg("line", { x1: 40, x2: 620, y1: 150, y2: 150, class: "history-grid" }), svg("text", { x: 0, y: 154 }, "0"));
        for (const row of known) {
            const value = measured(row, kind)!;
            const x = 40 + ((row.dayMs - minimum) / 86400000 + 0.5) / days * 580;
            const height = scale === 0 ? 0 : value / scale * 120;
            const width = Math.min(32, 580 / days * 0.6);
            const mark = svg("rect", { x: x - width / 2, y: 150 - height, width, height, "data-trend-value": value, fill: "#44892c" });
            mark.append(svg("title", {}, `${new Date(row.dayMs).toISOString().slice(0, 10)}: ${format(value)}`));
            chart.append(mark);
            if (value === 0)
                chart.append(svg("circle", { cx: x, cy: 150, r: 3, fill: "#44892c" }));
        }
        chart.append(svg("text", { x: 40, y: 175 }, new Date(minimum).toISOString().slice(0, 10)), svg("text", { x: 620, y: 175, "text-anchor": "end" }, new Date(maximum).toISOString().slice(0, 10)));
        figure.append(chart);
        section.append(figure);
    }
    const details = node("details");
    details.append(node("summary", `Observed ${title.toLowerCase()} rows (${rows.length})`));
    const table = node("table", "", "history-table");
    table.append(node("caption", "Original reported daily values and counts; source is local provider telemetry. Unknown timing is not converted to zero."));
    const head = node("thead"), heading = node("tr");
    for (const label of ["UTC day", "Provider / model / class", "Reported sum/value", "Reported count", "Chart measurement", "Coverage"]) {
        const th = node("th", label);
        th.scope = "col";
        heading.append(th);
    }
    head.append(heading);
    table.append(head);
    const body = node("tbody");
    for (const row of rows.slice(0, 365)) {
        const tr = node("tr");
        const value = measured(row, kind);
        const gap = view.history.events.some(event => event.provider === row.provider && event.metric === row.metric && event.model === row.model && event.dimension === row.dimension && Math.floor(event.endMs / 86400000) * 86400000 === row.dayMs && event.gapBefore);
        for (const text of [new Date(row.dayMs).toISOString().slice(0, 10), `${row.provider} / ${row.model === null ? "Not reported" : row.model} / ${row.dimension === null ? "Not reported" : row.dimension}`, format(row.value), row.count === null ? "Not reported" : String(row.count), value === null ? "Mean unavailable" : `${format(value)}${kind === "costUsd" ? " USD" : kind === "requestLatencyMs" ? " ms" : kind === "activeSeconds" ? " seconds" : kind === "apiRequests" ? " requests" : " tokens"}`, gap ? "Gap before reported interval" : "Captured exports only; completeness unknown"])
            tr.append(node("td", text));
        body.append(tr);
    }
    table.append(body);
    const scroll = node("div", "", "history-table-wrap");
    scroll.append(table);
    details.append(scroll);
    if (rows.length > 365)
        details.append(node("p", "Showing the first 365 reported rows; choose a shorter range for detail.", "quota-meta"));
    section.append(details);
    return section;
}
