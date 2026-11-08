import type { DashboardState } from "./types.ts";
export interface AccountUsage {
    source: "codexAppServer";
    scope: "account";
    observedMs: number;
    summary: {
        lifetimeTokens: number | null;
        peakDailyTokens: number | null;
        longestRunningTurnSec: number | null;
        currentStreakDays: number | null;
        longestStreakDays: number | null;
    };
    dailyUsageBuckets: {
        startDate: string;
        tokens: number;
    }[] | null;
}
export interface UsageSnapshot {
    data: AccountUsage | null;
    error: string | null;
}
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = ""): HTMLElementTagNameMap[K] { const element = document.createElement(tag); element.textContent = text; return element; }
export function renderAccountUsage(state: DashboardState): HTMLElement {
    const section = node("section");
    section.className = "history-section";
    section.append(node("h2", "Codex account activity"));
    section.append(node("p", "Source: Codex App Server · account-reported totals. These can include other devices and are separate from locally captured telemetry, quota percentages and API-equivalent cost estimates."));
    const usage = state.agents.find(agent => agent.agentId === "codex")?.accountUsage;
    if (!usage?.data) {
        section.append(node("p", usage?.error ? `Account activity unavailable (${usage.error}). Sign-in and quota results are shown separately.` : "Account activity has not been reported by a qualified Codex version. Unknown totals remain unavailable."));
        return section;
    }
    const data = usage.data, age = Date.now() - data.observedMs;
    section.append(node("p", `${state.lastRefreshMs === null ? "Saved snapshot" : age < 0 ? "Clock changed" : age >= 900000 ? "Stale snapshot" : "Reported snapshot"} · ${new Date(data.observedMs).toLocaleString()}`));
    const list = node("dl");
    const numbers = new Intl.NumberFormat();
    for (const [label, value, unit] of [["Lifetime tokens", data.summary.lifetimeTokens, "tokens"], ["Peak daily tokens", data.summary.peakDailyTokens, "tokens"], ["Longest running turn", data.summary.longestRunningTurnSec, "seconds"], ["Current streak", data.summary.currentStreakDays, "days"], ["Longest streak", data.summary.longestStreakDays, "days"]] as const)
        list.append(node("dt", label), node("dd", value === null ? "Not reported" : `${numbers.format(value)} ${unit}`));
    section.append(list);
    if (data.dailyUsageBuckets === null)
        section.append(node("p", "Daily totals not reported."));
    else if (!data.dailyUsageBuckets.length)
        section.append(node("p", "The provider reported no daily buckets."));
    else {
        const detail = node("details");
        const days = data.dailyUsageBuckets;
        detail.append(node("summary", `Daily account activity (${days.length} days)`));
        // Keep the saved numeric data, but allocate table nodes only while visible.
        detail.addEventListener("toggle", () => {
            if (!detail.hasAttribute("open")) {detail.querySelector("table")?.remove(); return;}
            if (detail.querySelector("table")) return;
            const table = node("table"), body = node("tbody");
            table.className = "history-table";
            for (const day of days) {
                const row = node("tr");
                row.append(node("th", day.startDate), node("td", numbers.format(day.tokens)));
                body.append(row);
            }
            table.append(node("caption", "Provider-reported tokens by day"), body);
            detail.append(table);
        });
        section.append(detail);
    }
    return section;
}
