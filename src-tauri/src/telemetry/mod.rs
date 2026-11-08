use serde::{Deserialize, Serialize};

mod accumulator;
mod parser;
mod request_count;
pub mod server;
mod validate;
pub use validate::validate_delta;

pub const MAX_BODY_BYTES: usize = 65_536;
pub const MAX_SERIES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    Claude,
    Gemini,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Metric {
    Tokens,
    CostUsd,
    ActiveSeconds,
    RequestLatencyMs,
    ApiRequests,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryError {
    Disabled,
    Io,
    TooLarge,
    Malformed,
    UnsupportedSignal,
    UnsupportedMetric,
    UnsupportedShape,
    IdentityUnavailable,
    OutOfRange,
    Conflict,
    OutOfOrder,
    Capacity,
    QueueFull,
    WorkerFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelemetryDelta {
    // Opaque diagnostic fingerprint; storage only, never display as identity.
    pub series_key: String,
    pub provider: Provider,
    pub metric: Metric,
    pub dimension: Option<String>,
    pub model: Option<String>,
    pub start_ms: i64,
    pub end_ms: i64,
    pub start_ns: String,
    pub end_ns: String,
    pub counter_start_ns: String,
    pub value: f64,
    pub count: Option<u64>,
    pub gap_before: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestResult {
    pub deltas: Vec<TelemetryDelta>,
    pub baseline_points: usize,
    pub duplicate_points: usize,
    pub ignored_metrics: usize,
    pub coverage_since_collector_start_only: bool,
}

#[derive(Clone, Default)]
pub struct TelemetryEngine {
    series: std::collections::HashMap<u64, parser::Point>,
}

impl TelemetryEngine {
    pub fn ingest_json(&mut self, input: &[u8]) -> Result<IngestResult, TelemetryError> {
        let (points, ignored_metrics) = parser::parse(input)?;
        let mut series = self.series.clone();
        let mut result = IngestResult {
            deltas: Vec::new(),
            baseline_points: 0,
            duplicate_points: 0,
            ignored_metrics,
            coverage_since_collector_start_only: true,
        };
        for point in points {
            accumulator::apply(&mut series, point, &mut result)?;
        }
        self.series = series;
        Ok(result)
    }
}
