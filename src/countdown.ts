export function quotaCountdownText(resetAtMs: number | null, observedMs: number, captured: boolean, now: number): string {
    if (resetAtMs === null)
        return "Reset not reported";
    if (observedMs > now)
        return "Clock changed · Reset unverified";
    const remaining = resetAtMs - now;
    if (remaining <= 0)
        return captured ? "Reset time passed · Waiting for fresh Claude capture" : "Reset time passed · Waiting for fresh quota";
    const seconds = Math.ceil(remaining / 1000);
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor(seconds / 3600) % 24;
    const minutes = Math.floor(seconds / 60) % 60;
    const rest = seconds % 60;
    return `Resets in ${days ? `${days}d ` : ""}${days || hours ? `${hours}h ` : ""}${days || hours || minutes ? `${minutes}m ` : ""}${rest}s`;
}
export function renderQuotaCountdown(resetAtMs: number | null, observedMs: number, captured: boolean): HTMLElement { const span = document.createElement("span"); span.dataset.resetCountdown = ""; span.dataset.resetAtMs = resetAtMs === null ? "" : String(resetAtMs); span.dataset.observedMs = String(observedMs); span.dataset.captured = String(captured); span.textContent = quotaCountdownText(resetAtMs, observedMs, captured, Date.now()); return span; }
export function updateQuotaCountdowns(root: HTMLElement, now = Date.now()): void {
    if (document.hidden) return;
    const height = document.documentElement.clientHeight;
    root.querySelectorAll<HTMLElement>("[data-reset-countdown]").forEach(span => {
        const bounds = span.getBoundingClientRect();
        // Offscreen seconds cannot be seen; the next visible tick catches up from the timestamp.
        if (bounds.bottom <= 0 || bounds.top >= height) return;
        const reset = span.dataset.resetAtMs === "" ? null : Number(span.dataset.resetAtMs);
        const text = quotaCountdownText(reset, Number(span.dataset.observedMs), span.dataset.captured === "true", now);
        if (span.textContent !== text) span.textContent = text;
    });
}
export function createCountdownUpdater(root: HTMLElement) { let timer: ReturnType<typeof setInterval> | null = null; let disposed = false; function visibility() { if (timer !== null) {
    clearInterval(timer);
    timer = null;
} if (disposed || document.hidden)
    return; updateQuotaCountdowns(root); timer = setInterval(() => updateQuotaCountdowns(root), 1000); } document.addEventListener("visibilitychange", visibility); visibility(); return { dispose() { disposed = true; if (timer !== null)
        clearInterval(timer); document.removeEventListener("visibilitychange", visibility); } }; }
