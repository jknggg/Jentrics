use jentrics_core::{
    storage::{Storage, StorageError},
    telemetry::{Metric, Provider, TelemetryDelta},
};
fn event() -> TelemetryDelta {
    TelemetryDelta {
        provider: Provider::Claude,
        metric: Metric::Tokens,
        dimension: Some("input".into()),
        model: Some("claude-test".into()),
        start_ms: 1,
        end_ms: 2,
        value: 10.0,
        count: None,
        gap_before: false,
        series_key: "defaultHasherV1:0123456789abcdef".into(),
        start_ns: "1000000".into(),
        end_ns: "2000000".into(),
        counter_start_ns: "1000000".into(),
    }
}
#[test]
fn metrics_batch_is_deduplicated_atomic_retained_and_clearable() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let mut s = Storage::open(&dir.path().join("state.sqlite")).unwrap();
    let a = event();
    assert_eq!(
        s.record_metrics(std::slice::from_ref(&a), 10, 30, 365)
            .unwrap(),
        1
    );
    assert_eq!(
        s.record_metrics(std::slice::from_ref(&a), 10, 30, 365)
            .unwrap(),
        0
    );
    assert_eq!(s.metric_history(1, 100).unwrap().events.len(), 1);
    assert_eq!(s.metric_history(1, 100).unwrap().daily[0].value, 10.0);
    let mut b = a.clone();
    b.value = 11.0;
    assert_eq!(
        s.record_metrics(&[b], 10, 30, 365),
        Err(StorageError::InvalidState)
    );
    assert_eq!(s.metric_history(1, 100).unwrap().daily[0].value, 10.0);
    s.record_metrics(&[], 31 * 86400000, 30, 365).unwrap();
    let history = s.metric_history(0, 32 * 86400000).unwrap();
    assert!(history.events.is_empty());
    assert_eq!(history.daily[0].value, 10.0);
    s.clear_metrics().unwrap();
    assert!(s.metric_history(0, 32 * 86400000).unwrap().daily.is_empty());
}
#[test]
fn replay_gap_metadata_does_not_change_native_interval_identity() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let mut s = Storage::open(&dir.path().join("state.sqlite")).unwrap();
    let mut first = event();
    first.gap_before = true;
    s.record_metrics(&[first], 10, 30, 365).unwrap();
    assert_eq!(s.record_metrics(&[event()], 10, 30, 365).unwrap(), 0);
    assert_eq!(s.metric_history(0, 100).unwrap().daily[0].value, 10.0);
}
#[test]
fn stale_replay_after_fine_event_pruning_cannot_inflate_retained_daily_totals() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let mut s = Storage::open(&dir.path().join("state.sqlite")).unwrap();
    s.record_metrics(&[event()], 10, 30, 365).unwrap();
    s.record_metrics(&[], 31 * 86400000, 30, 365).unwrap();
    assert_eq!(
        s.record_metrics(&[event()], 31 * 86400000, 30, 365),
        Err(StorageError::InvalidState)
    );
    assert_eq!(
        s.metric_history(0, 32 * 86400000).unwrap().daily[0].value,
        10.0
    );
}
