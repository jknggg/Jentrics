use super::{history_write::parse_enum, settings, Storage, StorageError};
use crate::{
    history::{HistoryCheck, HistorySample, HistoryView},
    model::QuotaSource,
};
use rusqlite::params;

const MAX_ROWS: usize = 10_000;

impl Storage {
    pub fn history(&self, range_hours: u16, now_ms: i64) -> Result<HistoryView, StorageError> {
        settings::validate_timestamp(now_ms)?;
        let bin_ms: i64 = match range_hours {
            24 => 300_000,
            168 => 3_600_000,
            720 => 21_600_000,
            _ => return Err(StorageError::InvalidState),
        };
        let settings = self.history_settings()?;
        let from_ms = (now_ms - i64::from(range_hours) * 3_600_000).max(0);
        // Null observations remain separate from known values so a later successful
        // observation in the same bin cannot erase an unknown interval.
        let mut statement = self
            .connection
            .prepare(
                "
            WITH ranked AS (
                SELECT *, ROW_NUMBER() OVER (
                    PARTITION BY agent_id,bucket_id,observed_ms/?3,resets_at_ms,source,
                        used_percent IS NULL,window_minutes IS NULL
                    ORDER BY observed_ms DESC
                ) AS position FROM quota_samples WHERE observed_ms BETWEEN ?1 AND ?2
            ) SELECT agent_id,bucket_id,observed_ms,used_percent,window_minutes,resets_at_ms,source
              FROM ranked WHERE position=1 ORDER BY observed_ms DESC,agent_id,bucket_id LIMIT 10001
        ",
            )
            .map_err(|_| StorageError::Corrupt)?;
        let rows = statement
            .query_map(params![from_ms, now_ms, bin_ms], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(|_| StorageError::Corrupt)?;
        let mut samples = Vec::new();
        for row in rows {
            let (agent, bucket_id, observed_ms, used_percent, window_minutes, resets_at_ms, source) =
                row.map_err(|_| StorageError::Corrupt)?;
            let _: QuotaSource = parse_enum(source.clone())?;
            if bucket_id.is_empty()
                || bucket_id.len() > 138
                || settings::validate_timestamp(observed_ms).is_err()
                || used_percent
                    .is_some_and(|value| !value.is_finite() || !(0.0..=100.0).contains(&value))
                || window_minutes.is_some_and(|value| value <= 0 || value > 9_007_199_254_740_991)
                || resets_at_ms.is_some_and(|value| settings::validate_timestamp(value).is_err())
            {
                return Err(StorageError::Corrupt);
            }
            samples.push(HistorySample {
                agent_id: parse_enum(agent)?,
                bucket_id,
                observed_ms,
                used_percent,
                window_minutes,
                resets_at_ms,
                source,
            });
        }
        let mut statement = self.connection.prepare("
            WITH ranked AS (
                SELECT *,ROW_NUMBER() OVER (
                    PARTITION BY agent_id,observed_ms/?3,status,diagnostic_code ORDER BY observed_ms DESC
                ) AS position FROM collection_checks WHERE observed_ms BETWEEN ?1 AND ?2
            ) SELECT agent_id,observed_ms,status,diagnostic_code FROM ranked WHERE position=1
              ORDER BY observed_ms DESC,agent_id,status,diagnostic_code LIMIT 10001
        ").map_err(|_| StorageError::Corrupt)?;
        let rows = statement
            .query_map(params![from_ms, now_ms, bin_ms], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|_| StorageError::Corrupt)?;
        let mut checks = Vec::new();
        for row in rows {
            let (agent, observed_ms, status, diagnostic) =
                row.map_err(|_| StorageError::Corrupt)?;
            if !matches!(status.as_str(), "reported" | "unavailable" | "error")
                || settings::validate_timestamp(observed_ms).is_err()
            {
                return Err(StorageError::Corrupt);
            }
            checks.push(HistoryCheck {
                agent_id: parse_enum(agent)?,
                observed_ms,
                status,
                diagnostic_code: diagnostic.map(parse_enum).transpose()?,
            });
        }
        let truncated = samples.len() > MAX_ROWS || checks.len() > MAX_ROWS;
        samples.truncate(MAX_ROWS);
        checks.truncate(MAX_ROWS);
        samples.reverse();
        checks.reverse();
        Ok(HistoryView {
            range_hours,
            from_ms,
            until_ms: now_ms,
            retention_days: settings.retention_days,
            samples,
            checks,
            truncated,
            error: None,
        })
    }
}
