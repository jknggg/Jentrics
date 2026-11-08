export interface AlertSettings {
    enabled: boolean;
    threshold80: boolean;
    threshold95: boolean;
    nearReset: boolean;
    expectedReset: boolean;
    restored: boolean;
    errors: boolean;
    quietHours: {
        startMinutes: number;
        endMinutes: number;
    } | null;
}
export type AlertPermission = "granted" | "denied" | "prompt" | "unavailable";
export type WindowsNotificationState = "accepting" | "busy" | "notPresent" | "fullScreen" | "presentation" | "quietTime" | "storeApp" | "unavailable";
export interface AlertSettingsView {
    settings: AlertSettings;
    permission: AlertPermission;
    windowsState?: WindowsNotificationState;
    error: "settingsUnavailable" | "deliveryUnavailable" | "permissionUnavailable" | null;
}
export interface AlertsBridge {
    preview?: boolean;
    getAlertSettings?(): Promise<AlertSettingsView>;
    setAlertSettings?(settings: AlertSettings): Promise<AlertSettingsView>;
    requestAlertPermission?(): Promise<AlertSettingsView>;
    sendTestNotification?(): Promise<void>;
}
