use super::{Storage, StorageError};
use crate::history::HistorySettings;
use rusqlite::{params, Connection};

pub(super) const MAX_TIMESTAMP: i64 = 8_640_000_000_000_000;
pub(super) const DAY_MS: i64 = 86_400_000;

pub(super) fn validate_timestamp(value: i64) -> Result<(), StorageError> {
    if !(0..=MAX_TIMESTAMP).contains(&value) {
        return Err(StorageError::InvalidState);
    }
    Ok(())
}

pub(crate) fn validate_days(days: u16) -> Result<(), StorageError> {
    if !matches!(days, 7 | 30 | 90 | 180 | 365) {
        return Err(StorageError::InvalidState);
    }
    Ok(())
}

pub(super) fn read_settings(connection: &Connection) -> Result<HistorySettings, StorageError> {
    let days: u16 = connection
        .query_row(
            "SELECT retention_days FROM app_settings WHERE id=1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| StorageError::Corrupt)?;
    validate_days(days).map_err(|_| StorageError::Corrupt)?;
    Ok(HistorySettings {
        retention_days: days,
    })
}

pub(super) fn prune(connection: &Connection, now_ms: i64) -> Result<usize, StorageError> {
    validate_timestamp(now_ms)?;
    let settings = read_settings(connection)?;
    let cutoff = now_ms
        .saturating_sub(i64::from(settings.retention_days) * DAY_MS)
        .max(0);
    let samples = connection
        .execute("DELETE FROM quota_samples WHERE observed_ms < ?1", [cutoff])
        .map_err(|_| StorageError::Unavailable)?;
    let checks = connection
        .execute(
            "DELETE FROM collection_checks WHERE observed_ms < ?1",
            [cutoff],
        )
        .map_err(|_| StorageError::Unavailable)?;
    Ok(samples + checks)
}

impl Storage {
    pub fn history_settings(&self) -> Result<HistorySettings, StorageError> {
        read_settings(&self.connection)
    }

    pub fn set_history_retention(
        &mut self,
        days: u16,
        now_ms: i64,
    ) -> Result<HistorySettings, StorageError> {
        validate_days(days)?;
        validate_timestamp(now_ms)?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        // A missing settings row is corruption, not a request to silently recreate defaults.
        read_settings(&transaction)?;
        transaction
            .execute(
                "UPDATE app_settings SET retention_days=?1 WHERE id=1",
                params![days],
            )
            .map_err(|_| StorageError::Unavailable)?;
        prune(&transaction, now_ms)?;
        transaction
            .commit()
            .map_err(|_| StorageError::Unavailable)?;
        self.finish_maintenance(super::MaintenanceOperation::QuotaRetention);
        Ok(HistorySettings {
            retention_days: days,
        })
    }

    pub fn clear_history(&mut self) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        transaction
            .execute_batch("DELETE FROM quota_samples; DELETE FROM collection_checks;")
            .map_err(|_| StorageError::Unavailable)?;
        transaction
            .commit()
            .map_err(|_| StorageError::Unavailable)?;
        self.finish_maintenance(super::MaintenanceOperation::QuotaClear);
        Ok(())
    }
}
