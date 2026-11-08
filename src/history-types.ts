import type { AgentId, DiagnosticCode } from "./types.ts";
export type HistoryRange = 24 | 168 | 720;
export interface HistorySample {
    agentId: AgentId;
    bucketId: string;
    observedMs: number;
    usedPercent: number | null;
    windowMinutes: number | null;
    resetsAtMs: number | null;
    source: string;
}
export interface HistoryCheck {
    agentId: AgentId;
    observedMs: number;
    status: "reported" | "unavailable" | "error";
    diagnosticCode: DiagnosticCode | null;
}
export interface HistoryView {
    rangeHours: HistoryRange;
    fromMs: number;
    untilMs: number;
    retentionDays: number;
    samples: HistorySample[];
    checks: HistoryCheck[];
    truncated: boolean;
    error: DiagnosticCode | null;
}
export interface HistorySettings {
    retentionDays: number;
}
