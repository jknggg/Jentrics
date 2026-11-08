export interface MonitorSettings {
    paused: boolean;
    intervalMinutes: 5 | 10 | 15 | 30;
}
export interface MonitorSettingsView {
    settings: MonitorSettings | null;
    error: "settingsUnavailable" | "invalidSettings" | null;
    nextCheckMs: number | null;
    effectiveIntervalMinutes: number | null;
    lastClockChangeMs: number | null;
    runtimeReason?:"paused"|"settingsUnavailable"|"noQualifiedProviders"|null;
}
export interface MonitorBridge {
    preview?: boolean;
    getMonitorSettings?(): Promise<MonitorSettingsView>;
    setMonitorSettings?(settings: MonitorSettings): Promise<MonitorSettingsView>;
}
