import type {ConfigurationBridge} from "./configuration-types.ts";
import type {AdapterCatalogBridge} from "./adapter-catalog-types.ts";
import type {SettingsRequestBridge} from "./settings-navigation.ts";
import type {MetricBridge} from "./metrics-types.ts";
import type {PricingBridge} from "./pricing-types.ts";
import type {HistoryRange,HistorySettings,HistoryView} from "./history-types.ts";
import type {UsageSnapshot} from "./account-usage.ts";
import type {ClaudeBridgePlan,ClaudeBridgeStatus} from "./claude-types.ts";
import type {AlertsBridge} from "./alerts-types.ts";
import type {MonitorBridge} from "./monitor-types.ts";
export type AgentId = "claude" | "codex";
export type Availability = "notFound" | "detected" | "unsupported" | "probeFailed";
export type AuthState = "ready" | "notAuthenticated" | "unknown";
export type DiagnosticCode = "executableUnavailable" | "versionUnavailable" | "unsupportedVersion" | "authUnavailable" | "probeTimeout" | "probeOutputLimit" | "storageUnavailable" | "storageIncompatible" | "storageCorrupt" | "temporaryDirectoryUnavailable" | "quotaUnavailable" | "invalidHistoryRequest" | "historyConfirmationRequired";
export interface Capabilities { discovery: boolean; authReadiness: boolean; quotas: boolean; usage: boolean; sessionStart: boolean }
export interface AgentState { agentId: AgentId; version: string | null; availability: Availability; authState: AuthState; capabilities: Capabilities; observedMs: number; diagnosticCode: DiagnosticCode | null; quota: QuotaSnapshot | null; accountUsage?:UsageSnapshot|null }
export interface DashboardState { schemaVersion: 1; agents: AgentState[]; refreshing: boolean; lastRefreshMs: number | null; storageError: DiagnosticCode | null; collectionError: DiagnosticCode | null }
export interface Bridge extends AlertsBridge,MonitorBridge,MetricBridge,PricingBridge,ConfigurationBridge,AdapterCatalogBridge,SettingsRequestBridge { preview: boolean; getState(): Promise<DashboardState>; refresh(): Promise<DashboardState>; subscribe(listener: (state: DashboardState) => void): Promise<() => void>; prepareWake?(agentId:AgentId):Promise<WakeOffer>; confirmWake?(agentId:AgentId,confirmationId:string,confirmed:boolean):Promise<WakeOutcome>; getHistory?(rangeHours:HistoryRange):Promise<HistoryView>; getHistorySettings?():Promise<HistorySettings>; setHistoryRetention?(retentionDays:number):Promise<HistorySettings>; clearHistory?(confirmed:boolean):Promise<void>; getClaudeBridgeStatus?():Promise<ClaudeBridgeStatus>; prepareClaudeBridge?():Promise<ClaudeBridgePlan>; applyClaudeBridge?(confirmationId:string,confirmed:boolean):Promise<ClaudeBridgeStatus>; restoreClaudeBridge?(confirmed:boolean):Promise<ClaudeBridgeStatus> }
export interface QuotaWindow { bucketId: string; windowMinutes: number | null; usedPercent: number | null; resetsAtMs: number | null; observedMs: number }
export interface QuotaSnapshot { source: "codexAppServer"|"claudeStatusLine"; observedMs: number; windows: QuotaWindow[]; diagnosticCode: DiagnosticCode | null }
export type WakeReason = "notInstalled" | "unsupported" | "retryPolicyUnsupported" | "signInRequired" | "subscriptionRequired" | "activeWindow" | "weeklyOnly" | "busy" | "cooldown" | "stopped" | "confirmationRequired" | "prepareRequired" | "expiredConfirmation" | "providerUnavailable" | "timedOut" | "outputLimit" | "requestFailed" | "cancelled";
export interface WakeOffer { agentId:AgentId; enabled:boolean; reason:WakeReason|null; modelLabel:string; prompt:string; quotaUnverified:boolean; confirmationId:string|null }
export interface WakeOutcome { agentId:AgentId; status:"completed"|"failed"|"timedOut"|"cancelled"|"unavailable"; reason:WakeReason|null; quotaEffect:"newWindowObserved"|"unchanged"|"unverified"; dashboard:DashboardState }
