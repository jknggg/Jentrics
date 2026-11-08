import type { ClaudeBridgeIssue, ClaudeBridgePlan, ClaudeBridgeStatus } from "./claude-types.ts";
export interface ClaudeUiState {
    available: boolean;
    status: ClaudeBridgeStatus | null;
    loading: boolean;
    busy: boolean;
    message: string | null;
    error: string | null;
}
const issues: Record<ClaudeBridgeIssue, string> = { existingStatusLine: "Your existing Claude status line was preserved. Jentrics will not replace or wrap it. Automatic setup is unavailable.", helperUnavailable: "The Claude capture helper is unavailable. Setup is disabled.", settingsUnavailable: "Claude settings could not be read. No settings were changed.", unsafePath: "A required path cannot be used safely. No settings were changed.", changedSinceReview: "Claude settings changed since review. They have been preserved. Reload status before reviewing again.", invalidSettings: "Claude settings are invalid or unsupported. They have been preserved.", captureUnavailable: "The saved Claude capture could not be read. Quota remains unavailable.", backupConflict: "A settings backup already exists or cannot be verified. It has been preserved.", confirmationRequired: "Confirm the reviewed change before changing Claude settings.", notConfigured: "The Claude bridge is not configured.", expiredReview: "This setup review expired. Review the command again before confirming." };
export function claudeIssue(error: unknown): string { return typeof error === "string" && Object.hasOwn(issues, error) ? issues[error as ClaudeBridgeIssue] : "The Claude bridge operation could not be completed. Reload status to verify the current configuration."; }
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] { const result = document.createElement(tag); result.textContent = text; if (className)
    result.className = className; return result; }
export function renderClaude(state: ClaudeUiState, preview: boolean): HTMLElement {
    const section = node("section", "", "claude-section");
    section.setAttribute("aria-labelledby", "claude-title");
    const title = node("h2", "Claude usage capture");
    title.id = "claude-title";
    section.append(title, node("p", "Optional local capture of provider-reported usage limits from Claude Code's status line. Refresh reads the saved capture; it does not start a Claude session or send a model request.", "quota-meta"));
    if (preview)
        section.append(node("p", "Synthetic preview only. Setup and restore change preview memory; no Claude settings are written.", "quota-meta"));
    if (!state.available) {
        section.append(node("p", "Claude usage capture is unavailable in this connection.", "quota-meta"));
        return section;
    }
    const status = state.status;
    const statusLabel = state.loading ? "Loading Claude bridge status…" : status?.configured === true ? "Configured" : status?.configured === false ? "Not configured" : "Configuration unknown";
    const summary = node("p", statusLabel, "claude-status");
    summary.setAttribute("role", "status");
    summary.setAttribute("aria-live", "polite");
    section.append(summary);
    if (status) {
        if (status.issue)
            section.append(node("p", issues[status.issue], "error-notice"));
        if (status.settingsPath)
            section.append(node("p", `Settings: ${status.settingsPath}`, "quota-meta"));
        if (status.capturePath)
            section.append(node("p", `Local capture: ${status.capturePath}`, "quota-meta"));
        if (status.command) {
            const detail = node("details");
            detail.append(node("summary", "Configured capture command"), node("pre", status.command, "wake-prompt"));
            section.append(detail);
        }
        const age = status.lastCaptureMs === null ? null : Date.now() - status.lastCaptureMs;
        section.append(node("p", age === null ? "No capture yet. Usage limits may appear after a supported Claude session reports them." : age < 0 ? "Capture clock changed; freshness is unknown." : `Saved capture: ${new Date(status.lastCaptureMs!).toLocaleString()}${age > 900000 ? " · stale" : ""}. This is a saved observation, not a live quota check.`, "quota-meta"));
    }
    const actions = node("div", "", "history-controls");
    for (const [id, label, disabled] of [["claude-status", "Reload status", state.loading || state.busy], ["claude-review", state.busy ? "Working…" : "Review setup", state.loading || state.busy || !status?.canSetup], ["claude-restore", "Restore previous settings…", state.loading || state.busy || !status?.canRestore]] as const) {
        const button = node("button", label, "wake-button");
        button.id = id;
        button.type = "button";
        button.disabled = disabled;
        actions.append(button);
    }
    section.append(actions);
    if (state.message) {
        const message = node("p", state.message, "quota-meta");
        message.setAttribute("role", "status");
        message.setAttribute("aria-live", "polite");
        section.append(message);
    }
    if (state.error) {
        const error = node("p", state.error, "error-notice");
        error.setAttribute("role", "status");
        section.append(error);
    }
    return section;
}
export function renderClaudeDialog(kind: "setup" | "restore", review: ClaudeBridgePlan | ClaudeBridgeStatus, preview: boolean, confirm: () => void, cancel: () => void): HTMLElement {
    const overlay = node("div", "", "wake-overlay");
    const dialog = node("section", "", "wake-dialog");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-labelledby", "claude-dialog-title");
    dialog.setAttribute("aria-describedby", "claude-dialog-description");
    const title = node("h2", kind === "setup" ? "Enable Claude usage capture?" : "Restore previous Claude settings?");
    title.id = "claude-dialog-title";
    const description = node("div");
    description.id = "claude-dialog-description";
    description.append(node("p", `Settings: ${review.settingsPath}`));
    if (kind === "setup") {
        if (!("confirmationId" in review))
            throw new Error("Claude setup requires a reviewed plan");
        description.append(node("p", `Capture directory: ${review.capturePath}`), node("p", "Jentrics adds this status-line command and creates a local backup of the previous settings. An existing status line is refused and preserved."), node("pre", review.command, "wake-prompt"), node("p", "No model request is sent. Only reported numeric usage limits are captured; no prompt, transcript or tool details are retained. Limits can be absent until Claude reports them."));
    }
    else
        description.append(node("p", "Restore the local backup and remove the Jentrics status-line setup. If settings changed since setup, restoration is refused and your current settings are preserved. Saved quota observations and history remain."));
    if (preview)
        description.append(node("p", "Synthetic preview only. No provider settings will be written.", "wake-warning"));
    const actions = node("div", "", "wake-dialog-actions");
    const no = node("button", "Cancel", "wake-button");
    no.id = "claude-cancel";
    no.type = "button";
    no.addEventListener("click", cancel);
    const yes = node("button", kind === "setup" ? "Enable capture" : "Restore settings", "refresh-button");
    yes.id = "claude-confirm";
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
