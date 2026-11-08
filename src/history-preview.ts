import type { DashboardState } from "./types.ts";
import type { HistoryCheck, HistoryRange, HistorySample, HistoryView } from "./history-types.ts";
// Browser-only synthetic memory. No filesystem or native/provider operation.
export function previewHistory() {
    let retentionDays = 90;
    let samples: HistorySample[] = [];
    let checks: HistoryCheck[] = [];
    const now = Date.now();
    const step = 300000;
    for (let observedMs = now - 30 * 86400000; observedMs <= now; observedMs += step) {
        if (observedMs > now - 5 * 3600000 && observedMs < now - 4 * 3600000)
            continue;
        const resetsAtMs = Math.ceil(observedMs / 18000000) * 18000000;
        for (const bucketId of ["codex:primary", "codex:secondary"]) {
            const windowMinutes = bucketId.endsWith("primary") ? 300 : 10080;
            const reset = windowMinutes === 300 ? resetsAtMs : Math.ceil(observedMs / 604800000) * 604800000;
            samples.push({ agentId: "codex", bucketId, observedMs, usedPercent: observedMs === now - 2 * 3600000 ? null : Math.round((1 - (reset - observedMs) / (windowMinutes * 60000)) * 70), windowMinutes, resetsAtMs: reset, source: "Synthetic preview" });
        }
        checks.push({ agentId: "codex", observedMs, status: "reported", diagnosticCode: null });
    }
    checks.push({ agentId: "codex", observedMs: now - 270 * 60000, status: "unavailable", diagnosticCode: "quotaUnavailable" });
    function prune() { const since = Date.now() - retentionDays * 86400000; samples = samples.filter(sample => sample.observedMs >= since); checks = checks.filter(check => check.observedMs >= since); }
    return {
        getHistory: async (rangeHours: HistoryRange): Promise<HistoryView> => {
            const untilMs = Date.now();
            const fromMs = untilMs - rangeHours * 3600000;
            const binMs = (rangeHours === 24 ? 5 : rangeHours === 168 ? 60 : 360) * 60000;
            const selected = new Map<string, HistorySample>();
            const selectedChecks = new Map<string, HistoryCheck>();
            for (const sample of samples)
                if (sample.observedMs >= fromMs && sample.observedMs <= untilMs)
                    selected.set(`${sample.bucketId}:${Math.floor(sample.observedMs / binMs)}:${sample.resetsAtMs}`, sample);
            for (const check of checks)
                if (check.observedMs >= fromMs && check.observedMs <= untilMs)
                    selectedChecks.set(`${check.agentId}:${Math.floor(check.observedMs / binMs)}:${check.status}:${check.diagnosticCode}`, check);
            return { rangeHours, fromMs, untilMs, retentionDays, samples: [...selected.values()], checks: [...selectedChecks.values()], truncated: false, error: null };
        },
        getHistorySettings: async () => ({ retentionDays }),
        setHistoryRetention: async (days: number) => { if (![7, 30, 90, 180, 365].includes(days))
            throw new Error("Invalid preview retention"); retentionDays = days; prune(); return { retentionDays }; },
        clearHistory: async (confirmed: boolean) => { if (!confirmed)
            throw new Error("Preview confirmation required"); samples = []; checks = []; },
        record(state: DashboardState) { for (const agent of state.agents) {
            checks.push({ agentId: agent.agentId, observedMs: agent.observedMs, status: agent.quota ? "reported" : "unavailable", diagnosticCode: agent.diagnosticCode });
            for (const window of agent.quota?.windows ?? [])
                samples.push({ ...window, agentId: agent.agentId, source: "Synthetic preview" });
        } prune(); }
    };
}
