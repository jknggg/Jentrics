export type ClaudeBridgeIssue = "existingStatusLine" | "helperUnavailable" | "settingsUnavailable" | "unsafePath" | "changedSinceReview" | "invalidSettings" | "captureUnavailable" | "backupConflict" | "confirmationRequired" | "notConfigured" | "expiredReview";
export interface ClaudeBridgeStatus {
    configured: boolean | null;
    canSetup: boolean;
    canRestore: boolean;
    settingsPath: string | null;
    capturePath: string | null;
    command: string | null;
    lastCaptureMs: number | null;
    issue: ClaudeBridgeIssue | null;
}
export interface ClaudeBridgePlan {
    confirmationId: string;
    settingsPath: string;
    capturePath: string;
    command: string;
}
