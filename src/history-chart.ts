import type { HistorySample, HistoryView } from "./history-types.ts";
const ns = "http://www.w3.org/2000/svg";
export function renderHistoryChart(history: HistoryView, samples: HistorySample[], label: string): SVGSVGElement {
    const svg = document.createElementNS(ns, "svg");
    svg.setAttribute("viewBox", "0 0 640 180");
    svg.classList.add("history-chart");
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", `${label}: reported percent used over time. Gaps and resets are not connected. Original values are in the observation table.`);
    const title = document.createElementNS(ns, "title");
    title.textContent = `${label} · reported percent used`;
    svg.append(title);
    const binMs = (history.rangeHours === 24 ? 5 : history.rangeHours === 168 ? 60 : 360) * 60000;
    const x = (time: number) => 40 + (time - history.fromMs) / (history.untilMs - history.fromMs) * 580;
    const y = (percent: number) => 150 - percent * 1.25;
    const observationTimes = new Set(samples.map(sample => sample.observedMs));
    const checks = history.checks.filter(check => check.agentId === samples[0]?.agentId).sort((a, b) => a.observedMs - b.observedMs);
    let checkIndex = 0;
    let barrierMs = Number.NEGATIVE_INFINITY;
    for (const value of [0, 50, 100]) {
        const line = document.createElementNS(ns, "line");
        line.setAttribute("x1", "40");
        line.setAttribute("x2", "620");
        line.setAttribute("y1", String(y(value)));
        line.setAttribute("y2", String(y(value)));
        line.classList.add("history-grid");
        const text = document.createElementNS(ns, "text");
        text.setAttribute("x", "0");
        text.setAttribute("y", String(y(value) + 4));
        text.textContent = `${value}%`;
        svg.append(line, text);
    }
    let segment: HistorySample[] = [];
    function finish() { if (segment.length > 1) {
        const path = document.createElementNS(ns, "polyline");
        path.setAttribute("points", segment.map(sample => `${x(sample.observedMs)},${y(sample.usedPercent!)}`).join(" "));
        svg.append(path);
    } segment = []; }
    for (const sample of [...samples].sort((a, b) => a.observedMs - b.observedMs)) {
        while (checkIndex < checks.length && checks[checkIndex]!.observedMs <= sample.observedMs) {
            const check = checks[checkIndex++]!;
            if (check.status !== "reported" || check.diagnosticCode !== null || !observationTimes.has(check.observedMs))
                barrierMs = check.observedMs;
        }
        if (sample.usedPercent === null) {
            finish();
            continue;
        }
        const previous = segment.at(-1);
        // A missing reset/duration cannot establish that observations share a window.
        const knownWindow = sample.resetsAtMs !== null && sample.windowMinutes !== null;
        if (previous && (!knownWindow || previous.resetsAtMs === null || previous.windowMinutes === null || previous.resetsAtMs !== sample.resetsAtMs || previous.windowMinutes !== sample.windowMinutes || previous.source !== sample.source || sample.observedMs - previous.observedMs > binMs * 2 || barrierMs >= previous.observedMs))
            finish();
        const point = document.createElementNS(ns, "circle");
        point.setAttribute("cx", String(x(sample.observedMs)));
        point.setAttribute("cy", String(y(sample.usedPercent)));
        point.setAttribute("r", "3");
        const detail = document.createElementNS(ns, "title");
        detail.textContent = `${new Date(sample.observedMs).toLocaleString()}: ${sample.usedPercent}% · ${sample.source}`;
        point.append(detail);
        svg.append(point);
        segment.push(sample);
        if (!knownWindow)
            finish();
    }
    finish();
    for (const [position, time] of [[40, history.fromMs], [620, history.untilMs]]) {
        const text = document.createElementNS(ns, "text");
        text.setAttribute("x", String(position));
        text.setAttribute("y", "175");
        text.setAttribute("text-anchor", position === 40 ? "start" : "end");
        text.textContent = new Date(time!).toLocaleString();
        svg.append(text);
    }
    return svg;
}
