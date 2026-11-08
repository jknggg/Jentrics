import type { AppConfiguration, ConfigurationBridge, MaintenanceStatus } from "./configuration-types.ts";
import { configurationError, parseConfigurationFile, validateConfiguration } from "./configuration-validate.ts";
import { renderConfiguration, renderConfigurationImport } from "./configuration-ui.ts";
import type { ConfigurationUiState } from "./configuration-ui.ts";
export function createConfigurationController(root: HTMLElement, content: HTMLElement, bridge: ConfigurationBridge, afterApply: () => Promise<void>) {
    const element = document.createElement("div");
    const state: ConfigurationUiState = { preview: bridge.preview === true, diagnostics: null, diagnosticsError: false, available: !!bridge.exportConfiguration, canImport: !!bridge.importConfiguration, busy: false, reading: false, saved: null, error: null, message: null, maintenance: null, maintenanceError: false };
    let disposed = false;
    let readRevision = 0;
    let fileRevision = 0;
    let maintenanceRevision = 0;
    let diagnosticsRevision = 0;
    let dialog: HTMLElement | null = null;
    let pending: AppConfiguration | null = null;
    function paint() { if (disposed)
        return; const expanded = element.querySelector("details")?.open; const focus = element.contains(document.activeElement) ? document.activeElement?.id : undefined; element.replaceChildren(renderConfiguration(state)); const details = element.querySelector("details"); if (details)
        details.open = expanded === true; element.querySelector("#configuration-export")?.addEventListener("click", () => { void download(); }); element.querySelector("#configuration-reload")?.addEventListener("click", () => { void reload(); }); element.querySelector<HTMLInputElement>("#configuration-file")?.addEventListener("change", event => { const file = (event.currentTarget as HTMLInputElement).files?.[0]; if (file)
        void review(file); }); if (focus)
        element.querySelector<HTMLElement>(`#${focus}`)?.focus(); }
    async function reloadMaintenance() { if (disposed)
        return; const revision = ++maintenanceRevision; if (!bridge.getStorageMaintenance) {
        state.maintenanceError = true;
        paint();
        return;
    } try {
        const result: MaintenanceStatus = await bridge.getStorageMaintenance();
        if (disposed || revision !== maintenanceRevision)
            return;
        state.maintenance = result;
        state.maintenanceError = false;
    }
    catch {
        if (!disposed && revision === maintenanceRevision)
            state.maintenanceError = true;
    } if (!disposed && revision === maintenanceRevision)
        paint(); }
    async function reloadDiagnostics() { if (disposed)
        return; const current = ++diagnosticsRevision; if (!bridge.getDiagnosticsStatus) {
        state.diagnosticsError = true;
        paint();
        return;
    } try {
        const result = await bridge.getDiagnosticsStatus();
        if (disposed || current !== diagnosticsRevision)
            return;
        state.diagnostics = result;
        state.diagnosticsError = false;
    }
    catch {
        if (!disposed && current === diagnosticsRevision)
            state.diagnosticsError = true;
    } if (!disposed && current === diagnosticsRevision)
        paint(); }
    async function readSaved() { if (!bridge.exportConfiguration)
        throw new Error("Configuration connection unavailable"); return validateConfiguration(await bridge.exportConfiguration()); }
    async function reload() { if (disposed || state.busy)
        return; const revision = ++readRevision; try {
        const result = await readSaved();
        if (disposed || revision !== readRevision)
            return;
        state.saved = result;
        state.error = null;
    }
    catch (error) {
        if (!disposed && revision === readRevision)
            state.error = configurationError(error);
    } paint(); await Promise.all([reloadMaintenance(), reloadDiagnostics()]); }
    async function download() { if (disposed || state.busy)
        return; const revision = ++readRevision; state.busy = true; state.error = null; state.message = null; paint(); try {
        const result = await readSaved();
        if (disposed || revision !== readRevision)
            return;
        state.saved = result;
        const objectUrl = URL.createObjectURL(new Blob([JSON.stringify(result, null, 2)], { type: "application/json" }));
        const link = document.createElement("a");
        link.href = objectUrl;
        link.download = "jentrics-configuration.json";
        element.append(link);
        link.click();
        link.remove();
        setTimeout(() => URL.revokeObjectURL(objectUrl), 0);
        state.message = "Configuration download prepared. It includes manual price reference metadata; protect the exported file.";
    }
    catch (error) {
        if (!disposed && revision === readRevision)
            state.error = configurationError(error);
    }
    finally {
        state.busy = false;
        paint();
    } }
    function close() { dialog?.remove(); dialog = null; pending = null; content.removeAttribute("inert"); if (!disposed)
        element.querySelector<HTMLElement>("#configuration-file")?.focus(); }
    async function review(file: File) { if (disposed || state.busy)
        return; const revision = ++fileRevision; close(); state.reading = true; state.error = null; state.message = null; paint(); try {
        const result = await parseConfigurationFile(file);
        if (disposed || revision !== fileRevision)
            return;
        if (root.querySelector('[role="dialog"]')) {
            state.error = "Close the current dialog before reviewing an import.";
            return;
        }
        pending = result;
        dialog = renderConfigurationImport(result, bridge.preview === true, activation => { void apply(activation); }, close);
        root.append(dialog);
        content.setAttribute("inert", "");
        dialog.querySelector<HTMLElement>("#configuration-cancel")?.focus();
    }
    catch (error) {
        if (!disposed && revision === fileRevision)
            state.error = configurationError(error);
    }
    finally {
        if (!disposed && revision === fileRevision) {
            state.reading = false;
            paint();
        }
    } }
    async function apply(activation: boolean) { if (disposed || state.busy || !pending || !dialog || !activation || !bridge.importConfiguration)
        return; const configuration = pending; close(); ++readRevision; ++fileRevision; state.busy = true; state.error = null; state.message = null; paint(); try {
        const result = await bridge.importConfiguration(configuration, true);
        if (disposed)
            return;
        state.saved = validateConfiguration(result);
        state.message = "App configuration replaced. Verify collector, alert and monitoring status; provider configuration was not changed.";
        await afterApply();
    }
    catch (error) {
        if (disposed)
            return;
        state.error = configurationError(error);
        if (error === "runtimeUnavailable")
            await afterApply();
    }
    finally {
        state.busy = false;
        if (!disposed) {
            await Promise.all([reloadMaintenance(), reloadDiagnostics()]);
            paint();
            element.querySelector<HTMLElement>("#configuration-reload")?.focus();
        }
    } }
    paint();
    return { element, start: reload, reload, reloadMaintenance, refreshStatus: () => Promise.all([reloadMaintenance(), reloadDiagnostics()]), dispose() { disposed = true; ++readRevision; ++fileRevision; ++maintenanceRevision; ++diagnosticsRevision; dialog?.remove(); dialog = null; pending = null; content.removeAttribute("inert"); } };
}
