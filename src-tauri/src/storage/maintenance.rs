use super::Storage;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaintenanceOperation {
    QuotaRetention,
    QuotaClear,
    MetricRetention,
    MetricClear,
    ConfigurationImport,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaintenanceOutcome {
    #[default]
    NeverRun,
    NotNeeded,
    Compacted,
    DeferredTooLarge,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaintenanceError {
    Busy,
    Full,
    Unavailable,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceStatus {
    pub operation: Option<MaintenanceOperation>,
    pub data_committed: bool,
    pub outcome: MaintenanceOutcome,
    pub error: Option<MaintenanceError>,
    pub reclaimed_bytes: Option<u64>,
}
impl Storage {
    pub fn maintenance_status(&self) -> MaintenanceStatus {
        self.maintenance.clone()
    }

    pub(super) fn finish_maintenance(&mut self, operation: MaintenanceOperation) {
        // All callers invoke this after their data transaction committed. A
        // maintenance failure cannot undo or misreport that committed operation.
        self.maintenance = MaintenanceStatus {
            operation: Some(operation),
            data_committed: true,
            outcome: MaintenanceOutcome::NotNeeded,
            error: None,
            reclaimed_bytes: Some(0),
        };
        if let Err(error) = self.compact_if_needed() {
            self.maintenance.outcome = MaintenanceOutcome::Failed;
            self.maintenance.error = Some(error);
            self.maintenance.reclaimed_bytes = None;
        }
    }
    fn compact_if_needed(&mut self) -> Result<(), MaintenanceError> {
        let pages = self.page_stat("page_count")?;
        let free = self.page_stat("freelist_count")?;
        let size = self.page_stat("page_size")?;
        // Avoid a full rewrite for small holes or each sample. Limit the rewrite
        // to 64 MiB; larger databases remain explicitly deferred. No DB limit is
        // imposed on ordinary collection, and no host settings are changed.
        if free.saturating_mul(size) < 1024 * 1024 || free.saturating_mul(4) < pages {
            return Ok(());
        }
        if pages.saturating_mul(size) > 64 * 1024 * 1024 {
            self.maintenance.outcome = MaintenanceOutcome::DeferredTooLarge;
            self.maintenance.reclaimed_bytes = None;
            return Ok(());
        }
        self.checkpoint()?;
        self.connection.execute_batch("VACUUM;").map_err(classify)?;
        self.checkpoint()?;
        let after = self.page_stat("page_count")?;
        self.maintenance.outcome = MaintenanceOutcome::Compacted;
        self.maintenance.reclaimed_bytes = Some(pages.saturating_sub(after).saturating_mul(size));
        Ok(())
    }
    fn checkpoint(&self) -> Result<(), MaintenanceError> {
        let busy: u32 = self
            .connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .map_err(classify)?;
        if busy != 0 {
            return Err(MaintenanceError::Busy);
        }
        Ok(())
    }
    fn page_stat(&self, name: &str) -> Result<u64, MaintenanceError> {
        let value: u32 = self
            .connection
            .pragma_query_value(None, name, |row| row.get(0))
            .map_err(classify)?;
        Ok(u64::from(value))
    }
}

fn classify(error: rusqlite::Error) -> MaintenanceError {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            MaintenanceError::Busy
        }
        Some(rusqlite::ErrorCode::DiskFull) => MaintenanceError::Full,
        _ => MaintenanceError::Unavailable,
    }
}
