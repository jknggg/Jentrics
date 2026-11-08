export interface DiagnosticsStatus {
    code: "notStarted" | "healthy" | "initializationUnavailable" | "writeUnavailable" | "capacityReached" | "queueDropped" | "shutdownIncomplete" | "stopped";
    droppedRecords: number;
    writtenRecords: number;
}
