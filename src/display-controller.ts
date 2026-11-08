import type { ConfigurationBridge, DisplaySettings } from "./configuration-types.ts";
import type { DashboardState } from "./types.ts";
import { configurationError, validateDisplaySettings } from "./configuration-validate.ts";
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] { const result = document.createElement(tag); result.textContent = text; if (className)
    result.className = className; return result; }
export function createDisplayController(bridge: ConfigurationBridge, getState: () => DashboardState, onSaved: (settings: DisplaySettings) => void) {
    const element = document.createElement("div");
    let saved: DisplaySettings | null = null;
    let draft: DisplaySettings | null = null;
    let dirty = false;
    let busy = false;
    let disposed = false;
    let revision = 0;
    let error: string | null = null;
    let message: string | null = null;
    function paint() { if (disposed)
        return; const focus = element.contains(document.activeElement) ? document.activeElement?.id : undefined; const section = node("section", "", "display-section"); section.append(node("h2", "Primary quota windows"), node("p", "Choose the reported bucket to highlight for each provider. Other buckets stay visible. Missing selections stay unavailable until you choose another; elapsed reset times never select a replacement.", "quota-meta")); if (!bridge.getDisplaySettings) {
        section.append(node("p", "Primary bucket settings are unavailable in this connection.", "quota-meta"));
        element.replaceChildren(section);
        return;
    } if (draft) {
        const form = node("form", "", "history-controls");
        form.id = "display-settings";
        for (const id of ["claude", "codex"] as const) {
            const label = node("label", id === "claude" ? "Claude primary bucket" : "Codex primary bucket", "history-field");
            const select = node("select");
            select.id = `display-${id}`;
            select.disabled = busy;
            const available = getState().agents.find(agent => agent.agentId === id)?.quota?.windows.map(window => window.bucketId) ?? [];
            const selected = draft.primaryBuckets[id];
            for (const value of [null, ...available, ...(selected !== null && !available.includes(selected) ? [selected] : [])]) {
                const option = node("option", value === null ? "No primary bucket" : available.includes(value) ? value : `Saved bucket unavailable: ${value}`);
                option.value = value === null ? "" : value;
                option.selected = value === selected;
                select.append(option);
            }
            select.addEventListener("change", () => { if (draft) {
                draft.primaryBuckets[id] = select.value === "" ? null : select.value;
                dirty = true;
            } });
            label.append(select);
            form.append(label);
            if (selected !== null && !available.includes(selected))
                section.append(node("p", `Selected primary bucket unavailable for ${id}: ${selected}. No replacement was inferred.`, "error-notice"));
        }
        const save = node("button", busy ? "Saving…" : "Save primary windows", "wake-button");
        save.id = "display-save";
        save.type = "submit";
        save.disabled = busy || !bridge.setDisplaySettings;
        form.append(save);
        form.addEventListener("submit", event => { event.preventDefault(); void saveSelection(); });
        save.addEventListener("click", event => { event.preventDefault(); void saveSelection(); });
        section.append(form);
    } const reload = node("button", "Reload primary settings", "wake-button"); reload.type = "button"; reload.disabled = busy; reload.addEventListener("click", () => { void load(false); }); section.append(reload); for (const text of [error, message])
        if (text) {
            const notice = node("p", text, error === text ? "error-notice" : "quota-meta");
            notice.setAttribute("role", "status");
            section.append(notice);
        } element.replaceChildren(section); if (focus)
        element.querySelector<HTMLElement>(`#${focus}`)?.focus(); }
    async function load(preserveDraft = true) { if (disposed || busy || !bridge.getDisplaySettings)
        return; const current = ++revision; try {
        const result = validateDisplaySettings(await bridge.getDisplaySettings());
        if (disposed || current !== revision)
            return;
        saved = result;
        if (!preserveDraft || !dirty) {
            draft = structuredClone(result);
            dirty = false;
        }
        error = null;
        onSaved(result);
    }
    catch (reason) {
        if (!disposed && current === revision)
            error = configurationError(reason);
    } paint(); }
    async function saveSelection() { if (disposed || busy || !draft || !bridge.setDisplaySettings)
        return; const settings = validateDisplaySettings(draft); busy = true; ++revision; error = null; message = null; paint(); try {
        const result = validateDisplaySettings(await bridge.setDisplaySettings(settings));
        if (disposed)
            return;
        saved = result;
        draft = structuredClone(result);
        dirty = false;
        message = "Primary window selections saved.";
        onSaved(result);
    }
    catch (reason) {
        if (!disposed)
            error = configurationError(reason);
    }
    finally {
        busy = false;
        paint();
    } }
    paint();
    return { element, start: () => load(false), reload: () => load(true), refreshSnapshot: paint, settings: () => saved, dispose() { disposed = true; ++revision; } };
}
