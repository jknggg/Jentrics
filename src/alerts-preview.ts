import type { AlertSettings, AlertSettingsView } from "./alerts-types.ts";
export function previewAlerts() {
    let settings: AlertSettings = { enabled: false, threshold80: true, threshold95: true, nearReset: true, expectedReset: true, restored: true, errors: true, quietHours: null };
    function view(): AlertSettingsView { return { settings: { ...settings, quietHours: settings.quietHours ? { ...settings.quietHours } : null }, permission: "unavailable", windowsState:"unavailable", error: null }; }
    return { sendTestNotification:async()=>{ /* Explicit browser simulation; no OS notification API. */ },getAlertSettings: async () => view(), setAlertSettings: async (next: AlertSettings) => { if (next.quietHours && (next.quietHours.startMinutes < 0 || next.quietHours.startMinutes >= 1440 || next.quietHours.endMinutes < 0 || next.quietHours.endMinutes >= 1440 || next.quietHours.startMinutes === next.quietHours.endMinutes))
            throw new Error("Invalid synthetic quiet hours"); settings = { ...next, quietHours: next.quietHours ? { ...next.quietHours } : null }; return view(); }, requestAlertPermission: async (): Promise<AlertSettingsView> => ({ ...view(), error: "permissionUnavailable" }) };
}
