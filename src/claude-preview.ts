import type { ClaudeBridgePlan, ClaudeBridgeStatus } from "./claude-types.ts";
export function previewClaudeBridge() {
    let configured = false;
    let review: {
        plan: ClaudeBridgePlan;
        until: number;
    } | null = null;
    let serial = 0;
    function status(): ClaudeBridgeStatus { return { configured, canSetup: !configured, canRestore: configured, settingsPath: "Preview memory / Claude settings", capturePath: "Preview memory / numeric capture", command: configured ? "Synthetic preview only — no command is executed" : null, lastCaptureMs: null, issue: null }; }
    return { getClaudeBridgeStatus: async () => status(), prepareClaudeBridge: async () => { if (configured)
            throw "existingStatusLine"; const plan: ClaudeBridgePlan = { confirmationId: `synthetic-claude-${++serial}`, settingsPath: "Preview memory / Claude settings", capturePath: "Preview memory / numeric capture", command: "Synthetic preview only — no command is executed" }; review = { plan, until: Date.now() + 120000 }; return plan; }, applyClaudeBridge: async (confirmationId: string, confirmed: boolean) => { const pending = review; review = null; if (!confirmed)
            throw "confirmationRequired"; if (!pending || pending.plan.confirmationId !== confirmationId || Date.now() > pending.until)
            throw "expiredReview"; configured = true; return status(); }, restoreClaudeBridge: async (confirmed: boolean) => { if (!confirmed)
            throw "confirmationRequired"; if (!configured)
            throw "notConfigured"; configured = false; review = null; return status(); } };
}
