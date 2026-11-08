use super::AppState;
use crate::{
    metrics::{valid_settings, MetricHistory, MetricSettings},
    model::DiagnosticCode,
    storage::PreferenceKey,
    telemetry::{server::Collector, IngestResult, TelemetryError},
};
use serde::Serialize;
use std::sync::{Arc, Mutex};
#[derive(Default)]
pub(super) struct MetricControl {
    worker: Mutex<Option<Collector>>,
    status: Mutex<MetricStatus>,
}
#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricStatus {
    pub coverage_started_ms: Option<i64>,
    pub last_received_ms: Option<i64>,
    pub error: Option<String>,
    pub baseline_points: usize,
    pub duplicate_points: usize,
    pub ignored_metrics: usize,
    pub rejected_batches: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricView {
    pub settings: MetricSettings,
    pub running: bool,
    pub status: MetricStatus,
    pub history: PublicHistory,
    pub endpoint: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicHistory {
    pub events: Vec<PublicEvent>,
    pub daily: Vec<crate::metrics::DailyMetric>,
    pub truncated: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicEvent {
    provider: crate::telemetry::Provider,
    metric: crate::telemetry::Metric,
    dimension: Option<String>,
    model: Option<String>,
    start_ms: i64,
    end_ms: i64,
    value: f64,
    count: Option<u64>,
    gap_before: bool,
}
impl From<MetricHistory> for PublicHistory {
    fn from(value: MetricHistory) -> Self {
        Self {
            events: value
                .events
                .into_iter()
                .map(|v| PublicEvent {
                    provider: v.provider,
                    metric: v.metric,
                    dimension: v.dimension,
                    model: v.model,
                    start_ms: v.start_ms,
                    end_ms: v.end_ms,
                    value: v.value,
                    count: v.count,
                    gap_before: v.gap_before,
                })
                .collect(),
            daily: value.daily,
            truncated: value.truncated,
        }
    }
}
impl AppState {
    fn metric_settings(&self) -> Result<MetricSettings, DiagnosticCode> {
        let settings = self
            .load_preference(PreferenceKey::Telemetry)?
            .unwrap_or_default();
        if !valid_settings(&settings) {
            return Err(DiagnosticCode::StorageCorrupt);
        }
        Ok(settings)
    }
    pub fn metrics_view(&self, range_days: u16) -> Result<MetricView, DiagnosticCode> {
        if !matches!(range_days, 1 | 7 | 30 | 365) {
            return Err(DiagnosticCode::InvalidHistoryRequest);
        }
        let settings = self.metric_settings()?;
        let now = super::now_ms();
        let history = self
            .storage
            .lock()
            .expect("storage lock poisoned")
            .as_ref()
            .ok_or(DiagnosticCode::StorageUnavailable)?
            .metric_history((now - i64::from(range_days) * 86400000).max(0), now)
            .map_err(super::storage_code)?;
        let running = self
            .metrics
            .worker
            .lock()
            .expect("metrics worker lock poisoned")
            .as_ref()
            .is_some_and(Collector::is_running);
        let mut status = self
            .metrics
            .status
            .lock()
            .expect("metrics status lock poisoned")
            .clone();
        if settings.enabled && !running && status.error.is_none() {
            status.error = Some("Local telemetry collector is not running.".into());
        }
        Ok(MetricView {
            settings,
            running,
            status,
            history: history.into(),
            endpoint: crate::telemetry::server::ENDPOINT,
        })
    }
    pub fn save_metric_settings(
        self: &Arc<Self>,
        settings: MetricSettings,
    ) -> Result<MetricView, DiagnosticCode> {
        if !valid_settings(&settings) {
            return Err(DiagnosticCode::InvalidHistoryRequest);
        }
        self.save_preference(PreferenceKey::Telemetry, &settings)?;
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_mut()
            .ok_or(DiagnosticCode::StorageUnavailable)?
            .record_metrics(
                &[],
                super::now_ms(),
                settings.event_retention_days,
                settings.daily_retention_days,
            )
            .map_err(super::storage_code)?;
        self.reconcile_metrics();
        self.metrics_view(30)
    }
    pub fn clear_metric_history(&self, confirmed: bool) -> Result<(), DiagnosticCode> {
        if !confirmed {
            return Err(DiagnosticCode::HistoryConfirmationRequired);
        }
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_mut()
            .ok_or(DiagnosticCode::StorageUnavailable)?
            .clear_metrics()
            .map_err(super::storage_code)
    }
    pub fn start_saved_metrics(self: &Arc<Self>) {
        self.reconcile_metrics();
    }
    pub(super) fn reconcile_metrics(self: &Arc<Self>) {
        let settings = match self.metric_settings() {
            Ok(v) => v,
            Err(_) => {
                self.metrics
                    .status
                    .lock()
                    .expect("metrics status lock poisoned")
                    .error = Some(
                    "Telemetry settings could not be read; collector remains disabled.".into(),
                );
                return;
            }
        };
        let mut worker = self
            .metrics
            .worker
            .lock()
            .expect("metrics worker lock poisoned");
        if !settings.enabled {
            if let Some(mut active) = worker.take() {
                if let Err(error) = active.stop() {
                    self.metrics
                        .status
                        .lock()
                        .expect("metrics status lock poisoned")
                        .error = Some(format!("Collector shutdown failed: {error:?}"));
                    return;
                }
            }
            self.metrics
                .status
                .lock()
                .expect("metrics status lock poisoned")
                .error = None;
            return;
        }
        if worker.as_ref().is_some_and(Collector::is_running) {
            return;
        }
        if let Some(mut previous) = worker.take() {
            if let Err(error) = previous.stop() {
                self.metrics
                    .status
                    .lock()
                    .expect("metrics status lock poisoned")
                    .error = Some(format!("Collector stopped: {error:?}"));
            }
        }
        let state = Arc::downgrade(self);
        match Collector::start_with_sink(
            true,
            Arc::new(move |result| {
                state
                    .upgrade()
                    .ok_or(TelemetryError::WorkerFailed)?
                    .accept_metrics(result)
            }),
        ) {
            Ok(active) => {
                *worker = Some(active);
                *self
                    .metrics
                    .status
                    .lock()
                    .expect("metrics status lock poisoned") = MetricStatus {
                    coverage_started_ms: Some(super::now_ms()),
                    ..Default::default()
                };
            }
            Err(error) => {
                self.metrics
                    .status
                    .lock()
                    .expect("metrics status lock poisoned")
                    .error = Some(format!("Collector could not start: {error:?}"));
            }
        }
    }
    fn accept_metrics(
        &self,
        result: Result<IngestResult, TelemetryError>,
    ) -> Result<(), TelemetryError> {
        let batch = match result {
            Ok(batch) => batch,
            Err(error) => {
                {
                    let mut status = self
                        .metrics
                        .status
                        .lock()
                        .expect("metrics status lock poisoned");
                    status.rejected_batches += 1;
                    status.error = Some(format!("Telemetry protocol/transport error: {error:?}"));
                }
                self.publish();
                // The input was rejected by the receiver. Acknowledging that
                // its diagnostic was recorded is not acceptance of a batch.
                return Ok(());
            }
        };
        let accepted = (|| {
            let settings = self.metric_settings().map_err(|_| TelemetryError::Io)?;
            if !settings.enabled {
                return Err(TelemetryError::Disabled);
            }
            let now = super::now_ms();
            let inserted = self
                .storage
                .lock()
                .expect("storage lock poisoned")
                .as_mut()
                .ok_or(TelemetryError::Io)?
                .record_metrics(
                    &batch.deltas,
                    now,
                    settings.event_retention_days,
                    settings.daily_retention_days,
                )
                .map_err(|_| TelemetryError::Io)?;
            let mut status = self
                .metrics
                .status
                .lock()
                .expect("metrics status lock poisoned");
            status.last_received_ms = Some(now);
            status.error = None;
            status.baseline_points += batch.baseline_points;
            status.duplicate_points += batch.duplicate_points + batch.deltas.len() - inserted;
            status.ignored_metrics += batch.ignored_metrics;
            Ok(())
        })();
        if let Err(error) = accepted {
            let mut status = self
                .metrics
                .status
                .lock()
                .expect("metrics status lock poisoned");
            status.rejected_batches += 1;
            status.error = Some(format!("Telemetry batch rejected: {error:?}"));
        }
        self.publish();
        accepted
    }
    pub(super) fn stop_metrics(&self) {
        if let Some(mut worker) = self
            .metrics
            .worker
            .lock()
            .expect("metrics worker lock poisoned")
            .take()
        {
            if let Err(error) = worker.stop() {
                eprintln!("Jentrics metrics shutdown: {error:?}");
            }
        }
    }
}
