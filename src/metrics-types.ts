export interface MetricSettings {
    enabled: boolean;
    eventRetentionDays: number;
    dailyRetentionDays: number;
}
export type MetricRange = 1 | 7 | 30 | 365;
export interface MetricEvent {
    provider: "claude" | "gemini";
    metric: "tokens" | "costUsd" | "activeSeconds" | "requestLatencyMs" | "apiRequests";
    dimension: string | null;
    model: string | null;
    startMs: number;
    endMs: number;
    value: number;
    count: number | null;
    gapBefore: boolean;
}
export interface DailyMetric {
    dayMs: number;
    provider: string;
    metric: string;
    dimension: string | null;
    model: string | null;
    value: number;
    count: number | null;
}
export interface MetricView {
    settings: MetricSettings;
    running: boolean;
    status: {
        coverageStartedMs: number | null;
        lastReceivedMs: number | null;
        error: string | null;
        baselinePoints: number;
        duplicatePoints: number;
        ignoredMetrics: number;
        rejectedBatches: number;
    };
    history: {
        events: MetricEvent[];
        daily: DailyMetric[];
        truncated: boolean;
    };
    endpoint: string;
}
export interface MetricBridge {
    preview: boolean;
    getMetrics?(rangeDays: number): Promise<MetricView>;
    setMetricSettings?(settings: MetricSettings): Promise<MetricView>;
    clearMetricHistory?(confirmed: boolean): Promise<void>;
}
