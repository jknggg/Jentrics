import type {AgentId} from "./types.ts";

export interface DashboardWindowControls {
    setCompact(compact: boolean): Promise<void>;
    setPinned(pinned: boolean): Promise<void>;
    close(): Promise<void>;
    drag(): Promise<void>;
}
export function createDashboardPresentation(controls: DashboardWindowControls | undefined, paint: () => void) {
    let compact = false;
    let pinned = false;
    let busy = false;
    let error: string | null = null;
    let provider: AgentId = "codex";
    let disposed = false;
    let pending: Promise<void> | null = null;
    function report(reason: unknown) {
        error = reason instanceof Error ? reason.message : String(reason);
        if (!disposed) paint();
    }
    async function change(next: boolean) {
        if (pending) await pending;
        if (disposed || compact === next) return;
        busy = true; error = null; paint();
        pending = (async () => {
            try {
                await controls?.setCompact(next);
                if (disposed) return;
                compact = next;
                if (!compact) pinned = false;
                document.documentElement.classList.toggle("mini-mode", compact);
            } catch (reason) {report(reason);}
            finally {busy = false; pending = null; if (!disposed) paint();}
        })();
        await pending;
    }
    function bind(root: HTMLElement) {
        root.querySelector("#mini-open")?.addEventListener("click", () => {void change(true).then(() => root.querySelector<HTMLElement>("#mini-provider")?.focus());});
        root.querySelector("#mini-expand")?.addEventListener("click", () => {void change(false).then(() => root.querySelector<HTMLElement>("#mini-open")?.focus());});
        root.querySelector("#mini-close")?.addEventListener("click", () => {void (controls ? controls.close().catch(report) : change(false));});
        root.querySelector("#mini-pin")?.addEventListener("click", async () => {
            if (!controls || busy || disposed) return;
            busy = true; error = null; paint();
            try {await controls.setPinned(!pinned); pinned = !pinned;}
            catch (reason) {report(reason);}
            finally {busy = false; if (!disposed) {paint(); root.querySelector<HTMLElement>("#mini-pin")?.focus();}}
        });
        root.querySelector<HTMLSelectElement>("#mini-provider")?.addEventListener("change", event => {
            const value = (event.currentTarget as HTMLSelectElement).value;
            if (value === "codex" || value === "claude") {provider = value; paint(); root.querySelector<HTMLElement>("#mini-provider")?.focus();}
        });
        root.querySelector("#mini-drag")?.addEventListener("mousedown", event => {
            const mouse = event as MouseEvent;
            if (mouse.button === 0 && !(mouse.target as Element).closest("button") && controls) void controls.drag().catch(report);
        });
    }
    return {
        active: () => compact,
        view: () => ({provider, pinned, busy, error}), bind,
        expand: async () => {await change(false); if (compact) throw new Error(error ?? "Could not expand dashboard");},
        dispose() {disposed = true; document.documentElement.classList.remove("mini-mode");}
    };
}
