use super::{settings::validate_timestamp, Storage, StorageError};
use crate::{
    metrics::{DailyMetric, MetricHistory},
    telemetry::{validate_delta, TelemetryDelta},
};
use rusqlite::{params, OptionalExtension};
impl Storage {
    pub fn record_metrics(
        &mut self,
        events: &[TelemetryDelta],
        now_ms: i64,
        event_days: u16,
        daily_days: u16,
    ) -> Result<usize, StorageError> {
        validate_timestamp(now_ms)?;
        if events.len() > 2000
            || !crate::metrics::valid_settings(&crate::metrics::MetricSettings {
                enabled: true,
                event_retention_days: event_days,
                daily_retention_days: daily_days,
            })
        {
            return Err(StorageError::InvalidState);
        }
        let tx = self
            .connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        let mut inserted = 0;
        for event in events {
            validate_delta(event).map_err(|_| StorageError::InvalidState)?;
            if event.end_ms > now_ms
                || event.end_ms
                    < now_ms
                        .saturating_sub(i64::from(event_days) * 86400000)
                        .max(0)
            {
                return Err(StorageError::InvalidState);
            }
            let key = format!(
                "{}:{}:{}:{}",
                event.series_key, event.counter_start_ns, event.start_ns, event.end_ns
            );
            let payload = serde_json::to_string(event).map_err(|_| StorageError::InvalidState)?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT payload FROM local_metric_events WHERE event_key=?1",
                    [&key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| StorageError::Unavailable)?;
            if let Some(existing) = existing {
                let stored: TelemetryDelta =
                    serde_json::from_str(&existing).map_err(|_| StorageError::Corrupt)?;
                validate_delta(&stored).map_err(|_| StorageError::Corrupt)?;
                let mut replay = event.clone();
                replay.gap_before = stored.gap_before;
                if stored != replay {
                    return Err(StorageError::InvalidState);
                }
                continue;
            }
            tx.execute(
                "INSERT INTO local_metric_events VALUES(?1,?2,?3)",
                params![key, event.end_ms, payload],
            )
            .map_err(|_| StorageError::Unavailable)?;
            let provider =
                serde_json::to_value(event.provider).map_err(|_| StorageError::InvalidState)?;
            let metric =
                serde_json::to_value(event.metric).map_err(|_| StorageError::InvalidState)?;
            tx.execute("INSERT INTO daily_metrics VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(day_ms,provider,metric,dimension,model) DO UPDATE SET value=daily_metrics.value+excluded.value,count=CASE WHEN daily_metrics.count IS NULL OR excluded.count IS NULL THEN NULL ELSE daily_metrics.count+excluded.count END",params![event.end_ms/86400000*86400000,provider.as_str(),metric.as_str(),event.dimension.as_deref().unwrap_or(""),event.model.as_deref().unwrap_or(""),event.value,event.count.map(|v|v as i64)]).map_err(|_|StorageError::Unavailable)?;
            inserted += 1;
        }
        let deleted = prune_metrics(&tx, now_ms, event_days, daily_days)?;
        tx.commit().map_err(|_| StorageError::Unavailable)?;
        if deleted > 0 {
            self.finish_maintenance(super::MaintenanceOperation::MetricRetention);
        }
        Ok(inserted)
    }
    pub fn metric_history(
        &self,
        from_ms: i64,
        until_ms: i64,
    ) -> Result<MetricHistory, StorageError> {
        validate_timestamp(from_ms)?;
        validate_timestamp(until_ms)?;
        if from_ms > until_ms {
            return Err(StorageError::InvalidState);
        }
        let mut events = Vec::new();
        let mut statement=self.connection.prepare("SELECT payload FROM local_metric_events WHERE observed_ms BETWEEN ?1 AND ?2 ORDER BY observed_ms DESC,event_key LIMIT 10001").map_err(|_|StorageError::Corrupt)?;
        let rows = statement
            .query_map(params![from_ms, until_ms], |row| row.get::<_, String>(0))
            .map_err(|_| StorageError::Corrupt)?;
        for row in rows {
            let payload = row.map_err(|_| StorageError::Corrupt)?;
            if payload.len() > 4096 {
                return Err(StorageError::Corrupt);
            }
            let event: TelemetryDelta =
                serde_json::from_str(&payload).map_err(|_| StorageError::Corrupt)?;
            validate_delta(&event).map_err(|_| StorageError::Corrupt)?;
            events.push(event);
        }
        let mut daily = Vec::new();
        let mut statement=self.connection.prepare("SELECT day_ms,provider,metric,dimension,model,value,count FROM daily_metrics WHERE day_ms BETWEEN ?1 AND ?2 ORDER BY day_ms DESC,provider,metric,dimension,model LIMIT 10001").map_err(|_|StorageError::Corrupt)?;
        let rows = statement
            .query_map(params![from_ms / 86400000 * 86400000, until_ms], |row| {
                Ok(DailyMetric {
                    day_ms: row.get(0)?,
                    provider: row.get(1)?,
                    metric: row.get(2)?,
                    dimension: row.get::<_, Option<String>>(3)?.filter(|s| !s.is_empty()),
                    model: row.get::<_, Option<String>>(4)?.filter(|s| !s.is_empty()),
                    value: row.get(5)?,
                    count: row.get(6)?,
                })
            })
            .map_err(|_| StorageError::Corrupt)?;
        for row in rows {
            let row = row.map_err(|_| StorageError::Corrupt)?;
            if !row.value.is_finite()
                || row.value < 0.0
                || row.count.is_some_and(|v| v < 0)
                || !matches!(row.provider.as_str(), "claude" | "gemini")
                || !matches!(
                    row.metric.as_str(),
                    "tokens" | "costUsd" | "activeSeconds" | "requestLatencyMs" | "apiRequests"
                )
            {
                return Err(StorageError::Corrupt);
            }
            daily.push(row);
        }
        let truncated = events.len() > 10000 || daily.len() > 10000;
        events.truncate(10000);
        daily.truncate(10000);
        Ok(MetricHistory {
            events,
            daily,
            truncated,
        })
    }
    pub fn clear_metrics(&mut self) -> Result<(), StorageError> {
        let tx = self
            .connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        tx.execute_batch("DELETE FROM local_metric_events;DELETE FROM daily_metrics;")
            .map_err(|_| StorageError::Unavailable)?;
        tx.commit().map_err(|_| StorageError::Unavailable)?;
        self.finish_maintenance(super::MaintenanceOperation::MetricClear);
        Ok(())
    }
}

pub(super) fn prune_metrics(
    connection: &rusqlite::Connection,
    now_ms: i64,
    event_days: u16,
    daily_days: u16,
) -> Result<usize, StorageError> {
    let events = connection
        .execute(
            "DELETE FROM local_metric_events WHERE observed_ms < ?1",
            [now_ms
                .saturating_sub(i64::from(event_days) * 86400000)
                .max(0)],
        )
        .map_err(|_| StorageError::Unavailable)?;
    let cutoff = now_ms
        .saturating_sub(i64::from(daily_days) * 86400000)
        .max(0)
        / 86400000
        * 86400000;
    let daily = connection
        .execute("DELETE FROM daily_metrics WHERE day_ms < ?1", [cutoff])
        .map_err(|_| StorageError::Unavailable)?;
    Ok(events + daily)
}
