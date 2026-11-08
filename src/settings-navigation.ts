export interface SettingsRequestBridge {
    subscribeSettings?(listener: () => void): Promise<() => void>;
    takeSettingsRequest?(): Promise<boolean>;
}
export function createSettingsNavigation(root: HTMLElement, bridge: SettingsRequestBridge, beforeOpen: () => Promise<void> = async () => {}) {
    let disposed = false;
    let stop: (() => void) | undefined;
    function report(text: string) { if (disposed)
        return; const notice = document.createElement("p"); notice.className = "error-notice"; notice.setAttribute("role", "status"); notice.textContent = text; root.querySelector(".configuration-section")?.append(notice); }
    async function consume() { if (disposed || !bridge.takeSettingsRequest)
        return; try {
        const pending = await bridge.takeSettingsRequest();
        if (disposed || !pending)
            return;
        await beforeOpen();
        if(disposed)return;
        const target = root.querySelector<HTMLElement>("#configuration-title");
        target?.focus();
        target?.scrollIntoView?.({ block: "start", behavior: "auto" });
    }
    catch {
        report("The tray Settings request could not be read. Open App configuration below.");
    } }
    return { async start() { if (!bridge.subscribeSettings || !bridge.takeSettingsRequest)
            return; try {
            const unlisten = await bridge.subscribeSettings(() => { void consume(); });
            if (disposed) {
                unlisten();
                return;
            }
            stop = unlisten;
            await consume();
        }
        catch {
            report("Tray Settings navigation is unavailable. App configuration remains available below.");
        } }, dispose() { disposed = true; stop?.(); } };
}
