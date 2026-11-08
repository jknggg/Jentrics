use jentrics_core::{
    storage::Storage,
    telemetry::{Metric, TelemetryEngine, TelemetryError},
};
use serde_json::{json, Value};

fn request(
    status: Value,
    category: Option<&str>,
    start: u64,
    end: u64,
    value: f64,
    temporality: u8,
) -> Vec<u8> {
    let mut attributes = vec![
        json!({"key":"model","value":{"stringValue":"gemini-test"}}),
        json!({"key":"status_code","value":status}),
    ];
    if let Some(category) = category {
        attributes.push(json!({"key":"error_type","value":{"stringValue":category}}));
    }
    serde_json::to_vec(&json!({"resourceMetrics":[{
        "resource":{"attributes":[{"key":"session.id","value":{"stringValue":"PRIVATE_SESSION"}}]},
        "scopeMetrics":[{"metrics":[{"name":"gemini_cli.api.request.count","unit":"",
            "sum":{"isMonotonic":true,"aggregationTemporality":temporality,"dataPoints":[{
                "attributes":attributes,"startTimeUnixNano":start.to_string(),"timeUnixNano":end.to_string(),"asDouble":value
            }]}}]}]
    }]})).unwrap()
}

#[test]
fn numeric_request_counter_distinguishes_provider_errors_from_receiver_failures() {
    for (status, category, dimension) in [
        (json!({"intValue":"200"}), None, "success"),
        (json!({"stringValue":"ok"}), None, "success"),
        (json!({"stringValue":"429"}), None, "error"),
        (json!({"stringValue":"error"}), None, "error"),
        (
            json!({"intValue":200}),
            Some("PRIVATE_ERROR_CATEGORY"),
            "error",
        ),
        (json!({"stringValue":"PENDING"}), None, "unknown"),
    ] {
        let result = TelemetryEngine::default()
            .ingest_json(&request(status, category, 1_000_000, 2_000_000, 3.0, 1))
            .unwrap();
        assert_eq!(result.deltas.len(), 1);
        let delta = &result.deltas[0];
        assert_eq!(delta.metric, Metric::ApiRequests);
        assert_eq!(delta.dimension.as_deref(), Some(dimension));
        assert_eq!(delta.value, 3.0);
        assert_eq!(delta.count, None);
        let encoded = serde_json::to_string(&result).unwrap();
        assert!(!encoded.contains("PRIVATE_"));
        assert!(!encoded.contains("status_code") && !encoded.contains("error_type"));
    }
}

#[test]
fn error_categories_and_statuses_keep_independent_cumulative_baselines() {
    let mut engine = TelemetryEngine::default();
    for category in ["ApiError", "NetworkError"] {
        let body = request(
            json!({"stringValue":"error"}),
            Some(category),
            1_000_000,
            2_000_000,
            5.0,
            2,
        );
        let baseline = engine.ingest_json(&body).unwrap();
        assert!(baseline.deltas.is_empty());
        assert_eq!(baseline.baseline_points, 1);
    }
    let mut keys = Vec::new();
    for category in ["ApiError", "NetworkError"] {
        let body = request(
            json!({"stringValue":"error"}),
            Some(category),
            1_000_000,
            3_000_000,
            7.0,
            2,
        );
        let observed = engine.ingest_json(&body).unwrap();
        assert_eq!(observed.deltas[0].value, 2.0);
        keys.push(observed.deltas[0].series_key.clone());
        assert!(engine.ingest_json(&body).unwrap().deltas.is_empty());
    }
    assert_ne!(keys[0], keys[1]);
}

#[test]
fn invalid_counts_and_private_error_text_are_not_admitted() {
    for body in [
        request(
            json!({"intValue":"200"}),
            None,
            1_000_000,
            2_000_000,
            1.5,
            1,
        ),
        request(
            json!({"intValue":"999"}),
            None,
            1_000_000,
            2_000_000,
            1.0,
            1,
        ),
        request(
            json!({"stringValue":"error"}),
            Some("private message/path"),
            1_000_000,
            2_000_000,
            1.0,
            1,
        ),
        request(
            json!({"stringValue":"error","intValue":"500"}),
            None,
            1_000_000,
            2_000_000,
            1.0,
            1,
        ),
    ] {
        assert!(matches!(
            TelemetryEngine::default().ingest_json(&body),
            Err(TelemetryError::OutOfRange
                | TelemetryError::Malformed
                | TelemetryError::UnsupportedShape)
        ));
    }
}

#[test]
fn request_error_daily_totals_persist_replay_once_without_raw_categories() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("requests.sqlite");
    let mut storage = Storage::open(&path).unwrap();
    let mut engine = TelemetryEngine::default();
    for (offset, category) in [(0, "PRIVATE_ERROR_CATEGORY"), (1, "OtherError")] {
        let start = 1_000_000 + offset * 1_000_000;
        let result = engine
            .ingest_json(&request(
                json!({"intValue":"500"}),
                Some(category),
                start,
                start + 1_000_000,
                2.0,
                1,
            ))
            .unwrap();
        assert_eq!(storage.record_metrics(&result.deltas, 1000, 30, 365), Ok(1));
        assert_eq!(storage.record_metrics(&result.deltas, 1000, 30, 365), Ok(0));
    }
    let history = storage.metric_history(0, 1000).unwrap();
    assert_eq!(history.events.len(), 2);
    assert_eq!(history.daily.len(), 1);
    assert_eq!(history.daily[0].metric, "apiRequests");
    assert_eq!(history.daily[0].dimension.as_deref(), Some("error"));
    assert_eq!(history.daily[0].value, 4.0);
    assert!(!serde_json::to_string(&history)
        .unwrap()
        .contains("PRIVATE_"));
}
