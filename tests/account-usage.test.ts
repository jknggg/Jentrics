import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { renderAccountUsage } from "../src/account-usage.ts";
import type { DashboardState } from "../src/types.ts";
test("account usage preserves null, explicitzero and account-only scope", () => { const { document } = parseHTML("<html></html>"); Object.assign(globalThis, { document }); const state = { agents: [{ agentId: "codex", accountUsage: { data: { source: "codexAppServer", scope: "account", observedMs: Date.now(), summary: { lifetimeTokens: 0, peakDailyTokens: null, longestRunningTurnSec: null, currentStreakDays: 1, longestStreakDays: 1 }, dailyUsageBuckets: [] }, error: null } }], lastRefreshMs: Date.now() } as unknown as DashboardState; const text = renderAccountUsage(state).textContent!; assert.match(text, /0 tokens/); assert.match(text, /Not reported/); assert.match(text, /include other devices/); assert.match(text, /no daily buckets/); });

test("daily activity allocates rows only while expanded and releases them on collapse", () => {
    const {document, window} = parseHTML("<html></html>");
    Object.assign(globalThis, {document});
    const days = Array.from({length: 190}, (_, index) => ({startDate: `day-${index}`, tokens: index * 1000}));
    const state = {agents: [{agentId: "codex", accountUsage: {data: {
        source: "codexAppServer", scope: "account", observedMs: Date.now(),
        summary: {lifetimeTokens: 0, peakDailyTokens: null, longestRunningTurnSec: null, currentStreakDays: 1, longestStreakDays: 1},
        dailyUsageBuckets: days
    }, error: null}}], lastRefreshMs: Date.now()} as unknown as DashboardState;
    const section = renderAccountUsage(state);
    const detail = section.querySelector("details")!;
    assert.match(detail.querySelector("summary")!.textContent!, /190 days/);
    assert.equal(detail.querySelectorAll("tr").length, 0, "collapsed daily rows must not occupy the DOM");
    detail.setAttribute("open", ""); detail.dispatchEvent(new window.Event("toggle"));
    assert.equal(detail.querySelectorAll("tr").length, days.length);
    assert.equal(detail.querySelector("tr th")!.textContent, "day-0");
    assert.equal(detail.querySelector("tr td")!.textContent, "0");
    assert.equal(detail.querySelectorAll("tr")[189].lastElementChild!.textContent, new Intl.NumberFormat().format(189000));
    detail.dispatchEvent(new window.Event("toggle"));
    assert.equal(detail.querySelectorAll("tr").length, days.length, "repeated toggle events must not duplicate rows");
    detail.removeAttribute("open"); detail.dispatchEvent(new window.Event("toggle"));
    assert.equal(detail.querySelectorAll("table").length, 0, "collapsed rows must be released");
    detail.setAttribute("open", ""); detail.dispatchEvent(new window.Event("toggle"));
    assert.equal(detail.querySelectorAll("tr").length, days.length);
});
