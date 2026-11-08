export interface SourceCapabilities {
    readiness: boolean;
    quotaWindows: boolean;
    resetTimestamps: boolean;
    tokens: boolean;
    accountUsage: boolean;
    modelIds: boolean;
    requestDuration: boolean;
    meanRequestLatency: boolean;
    activeTime: boolean;
    timeToFirstToken: boolean;
    outputRate: boolean;
    providerErrorCounts: boolean;
    incrementalUsageCursor: boolean;
    startSession: boolean;
}
export interface AdapterInterface {
    adapterId: "claude" | "codex" | "gemini" | "opencode";
    adapterVersion: 1;
    source: "claudeAuthStatus" | "claudeStatusLine" | "claudeOtlpHttpJson" | "codexAppServer" | "geminiOtlpHttpJson" | "openCodeStatsV2";
    cliVersion: string | null;
    qualification: "unchecked" | "notInstalled" | "unsupportedVersion" | "readOnlyProfile" | "fixtureQualified" | "sourceContractOnly" | "startupIsolationUnqualified" | "interfaceUnavailable";
    requiresOptIn: boolean;
    nativeCollectionAllowed: boolean;
    capabilities: SourceCapabilities;
}
export interface AdapterCatalog {
    schemaVersion: 1;
    interfaces: AdapterInterface[];
}
export interface AdapterCatalogBridge {
    preview?: boolean;
    getAdapterCatalog?(): Promise<AdapterCatalog>;
}
