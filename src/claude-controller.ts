import type { Bridge } from "./types.ts";
import type { ClaudeBridgePlan } from "./claude-types.ts";
import { claudeIssue, renderClaude, renderClaudeDialog } from "./claude-ui.ts";
import type { ClaudeUiState } from "./claude-ui.ts";
export function createClaudeController(root: HTMLElement, content: HTMLElement, bridge: Bridge) {
    const element = document.createElement("div");
    const state: ClaudeUiState = { available: !!bridge.getClaudeBridgeStatus, status: null, loading: false, busy: false, message: null, error: null };
    let disposed = false;
    let revision = 0;
    let dialog: HTMLElement | null = null;
    let returnFocus = "claude-review";
    function paint() { if (disposed)
        return; const focusId = element.contains(document.activeElement) ? document.activeElement?.id : undefined; element.replaceChildren(renderClaude(state, bridge.preview)); element.querySelector("#claude-status")?.addEventListener("click", () => { void load(); }); element.querySelector("#claude-review")?.addEventListener("click", () => { void prepare(); }); element.querySelector("#claude-restore")?.addEventListener("click", restore); if (focusId)
        element.querySelector<HTMLElement>(`#${focusId}`)?.focus(); }
    function close() { dialog?.remove(); dialog = null; content.removeAttribute("inert"); if (!disposed)
        element.querySelector<HTMLElement>(`#${returnFocus}`)?.focus(); }
    async function load() { if (disposed || state.busy || !bridge.getClaudeBridgeStatus)
        return; const current = ++revision; state.loading = true; state.error = null; paint(); try {
        const result = await bridge.getClaudeBridgeStatus();
        if (!disposed && current === revision)
            state.status = result;
    }
    catch {
        if (!disposed && current === revision) {
            state.status = null;
            state.error = "Claude bridge status could not be loaded. Reload status to try again.";
        }
    }
    finally {
        if (!disposed && current === revision) {
            state.loading = false;
            paint();
        }
    } }
    async function prepare() {
        if (disposed || state.busy || !state.status?.canSetup || dialog || root.querySelector('[role="dialog"]'))
            return;
        if (!bridge.prepareClaudeBridge || !bridge.applyClaudeBridge) {
            state.error = "Claude setup is unavailable in this connection.";
            paint();
            return;
        }
        state.busy = true;
        state.error = null;
        state.message = null;
        returnFocus = "claude-review";
        paint();
        try {
            const review = await bridge.prepareClaudeBridge();
            if (disposed)
                return;
            if (!review.confirmationId) {
                state.error = "Claude setup review could not be verified. No settings were changed.";
                return;
            }
            dialog = renderClaudeDialog("setup", review, bridge.preview, () => { void apply(review); }, close);
            root.append(dialog);
            content.setAttribute("inert", "");
            dialog.querySelector<HTMLElement>("#claude-cancel")?.focus();
        }
        catch (error) {
            if (!disposed)
                state.error = claudeIssue(error);
        }
        finally {
            state.busy = false;
            paint();
            if (!dialog && !disposed)
                element.querySelector<HTMLElement>("#claude-review")?.focus();
        }
    }
    async function apply(review: ClaudeBridgePlan) { if (disposed || !dialog || state.busy || !bridge.applyClaudeBridge)
        return; close(); state.busy = true; state.error = null; ++revision; paint(); try {
        const result = await bridge.applyClaudeBridge(review.confirmationId, true);
        if (disposed)
            return;
        state.status = result;
        state.message = result.issue ? null : "Claude capture configured. Usage remains unavailable until a supported session reports limits.";
    }
    catch (error) {
        if (!disposed)
            state.error = claudeIssue(error);
    }
    finally {
        state.busy = false;
        paint();
        if (!disposed)
            element.querySelector<HTMLElement>("#claude-status")?.focus();
    } }
    function restore() { if (disposed || state.busy || !state.status?.canRestore || dialog || root.querySelector('[role="dialog"]'))
        return; if (!bridge.restoreClaudeBridge) {
        state.error = "Claude restore is unavailable in this connection.";
        paint();
        return;
    } returnFocus = "claude-restore"; dialog = renderClaudeDialog("restore", state.status, bridge.preview, () => { void confirmRestore(); }, close); root.append(dialog); content.setAttribute("inert", ""); dialog.querySelector<HTMLElement>("#claude-cancel")?.focus(); }
    async function confirmRestore() { if (disposed || !dialog || state.busy || !bridge.restoreClaudeBridge)
        return; close(); state.busy = true; state.error = null; state.message = null; ++revision; paint(); try {
        const result = await bridge.restoreClaudeBridge(true);
        if (disposed)
            return;
        state.status = result;
        state.message = result.issue ? null : "Previous Claude settings restored. Saved observations remain.";
    }
    catch (error) {
        if (!disposed)
            state.error = claudeIssue(error);
    }
    finally {
        state.busy = false;
        paint();
        if (!disposed)
            element.querySelector<HTMLElement>("#claude-status")?.focus();
    } }
    paint();
    return { element, start: load, dispose() { disposed = true; ++revision; dialog?.remove(); dialog = null; content.removeAttribute("inert"); } };
}
