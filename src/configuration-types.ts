import type { MonitorSettings } from "./monitor-types.ts";
import type { AlertSettings } from "./alerts-types.ts";
import type { MetricSettings } from "./metrics-types.ts";
import type { ModelPrice } from "./pricing-types.ts";
import type { DiagnosticsStatus } from "./diagnostics-types.ts";
export interface DisplaySettings {
    primaryBuckets: {
        claude: string | null;
        codex: string | null;
    };
}
export interface AppConfiguration {
    schemaVersion: 1;
    monitoring: MonitorSettings;
    alerts: AlertSettings;
    telemetry: MetricSettings;
    quotaRetentionDays: number;
    display: DisplaySettings;
    priceOverrides: ModelPrice[];
}
export type ConfigurationError = "confirmationRequired" | "unsupportedSchema" | "invalidConfiguration" | "tooLarge" | "storageUnavailable" | "storageCorrupt" | "runtimeUnavailable";
export interface MaintenanceStatus {
    operation: null | "quotaRetention" | "quotaClear" | "metricRetention" | "metricClear" | "configurationImport";
    dataCommitted: boolean;
    outcome: "neverRun" | "notNeeded" | "compacted" | "deferredTooLarge" | "failed";
    error: null | "busy" | "full" | "unavailable";
    reclaimedBytes: number | null;
}
export interface ConfigurationBridge {
    preview?: boolean;
    exportConfiguration?(): Promise<AppConfiguration>;
    importConfiguration?(configuration: AppConfiguration, confirmed: boolean): Promise<AppConfiguration>;
    getDisplaySettings?(): Promise<DisplaySettings>;
    setDisplaySettings?(settings: DisplaySettings): Promise<DisplaySettings>;
    getStorageMaintenance?(): Promise<MaintenanceStatus>;
    getDiagnosticsStatus?(): Promise<DiagnosticsStatus>;
}
