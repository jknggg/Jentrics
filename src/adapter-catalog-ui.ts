import type { AdapterCatalog, AdapterCatalogBridge, AdapterInterface, SourceCapabilities } from "./adapter-catalog-types.ts";
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] { const result = document.createElement(tag); result.textContent = text; if (className)
    result.className = className; return result; }
const qualifications: Record<AdapterInterface["qualification"], string> = { unchecked: "Not checked", notInstalled: "Not installed", unsupportedVersion: "Unsupported version", readOnlyProfile: "Qualified read-only profile", fixtureQualified: "Fixture-qualified contract", sourceContractOnly: "Source contract only · live export unverified", startupIsolationUnqualified: "Startup isolation unqualified", interfaceUnavailable: "Interface unavailable" };
const capabilityNames: Record<keyof SourceCapabilities, string> = { readiness: "Readiness", quotaWindows: "Quota windows", resetTimestamps: "Reset timestamps", tokens: "Tokens", accountUsage: "Account activity", modelIds: "Model IDs", requestDuration: "Per-request duration", meanRequestLatency: "Mean request latency", activeTime: "Active time", timeToFirstToken: "First-token timing", outputRate: "Output rate", providerErrorCounts: "Provider error counts", incrementalUsageCursor: "Incremental usage cursor", startSession: "Session start" };
export function renderAdapterCatalog(catalog: AdapterCatalog | null, error: string | null, preview: boolean): HTMLElement { const section = node("section", "", "adapter-catalog-section"); section.append(node("h2", "Data sources"), node("p", "Capabilities describe the admitted source contract, not a promised current measurement. Source-qualified telemetry does not establish a live CLI version or a completed export. Receiver transport failures are separate from provider request errors.", "quota-meta")); if (preview)
    section.append(node("p", "Synthetic preview only. This catalog does not probe or collect from native providers.", "quota-meta")); const reload = node("button", "Reload data-source status", "wake-button"); reload.id = "adapter-catalog-reload"; reload.type = "button"; section.append(reload); if (error) {
    const notice = node("p", error, "error-notice");
    notice.setAttribute("role", "status");
    section.append(notice);
} if (!catalog) {
    section.append(node("p", "Data-source capabilities are unavailable in this connection.", "quota-meta"));
    return section;
} const details = node("details"); details.append(node("summary", `Source capabilities and limits (${catalog.interfaces.length})`)); const table = node("table", "", "history-table"); table.append(node("caption", "Reported source contracts; unavailable fields remain explicit.")); const head = node("thead"), tr = node("tr"); for (const label of ["Source", "Qualification", "Collection", "Capabilities and limits"]) {
    const th = node("th", label);
    th.scope = "col";
    tr.append(th);
} head.append(tr); table.append(head); const body = node("tbody"); for (const source of catalog.interfaces) {
    const row = node("tr");
    row.append(node("td", `${source.adapterId} / ${source.source} · interface ${source.adapterVersion}`), node("td", `${qualifications[source.qualification]} · ${source.cliVersion === null ? "CLI version not checked" : source.cliVersion}`), node("td", `${source.nativeCollectionAllowed ? "Native collection allowed by this profile" : "Native collection unavailable"}${source.requiresOptIn ? " · Separate opt-in required" : ""}`));
    const capabilities = node("td");
    const keys = Object.keys(capabilityNames) as (keyof SourceCapabilities)[];
    capabilities.append(node("p", `Admitted fields: ${keys.filter(key => source.capabilities[key]).map(key => capabilityNames[key]).join(", ") || "None"}`), node("p", `Unavailable: ${keys.filter(key => !source.capabilities[key]).map(key => capabilityNames[key]).join(", ") || "None declared"}`));
    row.append(capabilities);
    body.append(row);
} table.append(body); const scroll = node("div", "", "history-table-wrap"); scroll.append(table); details.append(scroll); section.append(details); return section; }
export function createAdapterCatalogController(bridge: AdapterCatalogBridge) { const element = document.createElement("div"); let catalog: AdapterCatalog | null = null; let error: string | null = null; let disposed = false; let revision = 0; function paint() { if (disposed)
    return; element.replaceChildren(renderAdapterCatalog(catalog, error, bridge.preview === true)); element.querySelector("#adapter-catalog-reload")?.addEventListener("click", () => { void reload(); }); } async function reload() { if (disposed || !bridge.getAdapterCatalog)
    return; const current = ++revision; try {
    const next = await bridge.getAdapterCatalog();
    if (disposed || current !== revision)
        return;
    catalog = next;
    error = null;
}
catch {
    if (!disposed && current === revision)
        error = "Data-source status could not be read. No capabilities were inferred.";
} paint(); } paint(); return { element, start: reload, reload, dispose() { disposed = true; ++revision; } }; }
