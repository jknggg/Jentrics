use super::{Storage, StorageError};
use crate::telemetry::{Metric, Provider, TelemetryDelta};

// Fault injection through SQLite's real page allocator, not a host low-disk benchmark.
#[test]
fn sqlite_full_rolls_back_metrics_and_retry_persists_exactly_once() {
    let started = std::time::Instant::now();
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("disk-full.sqlite");
    let mut storage = Storage::open(&path).unwrap();
    let baseline = event(0, 10.0);
    assert_eq!(storage.record_metrics(&[baseline], 2000, 30, 365), Ok(1));
    let prior = storage.metric_history(0, 2000).unwrap();
    assert_eq!(prior.events.len(), 1);
    assert_eq!(prior.daily.len(), 1);
    assert_eq!(prior.daily[0].value, 10.0);
    let prior = serde_json::to_value(prior).unwrap();
    let mode: String = storage
        .connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    let original_limit: i64 = storage
        .connection
        .pragma_query_value(None, "max_page_count", |row| row.get(0))
        .unwrap();
    let pages: i64 = storage
        .connection
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .unwrap();
    let page_size: i64 = storage
        .connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .unwrap();
    storage
        .connection
        .pragma_update(None, "max_page_count", pages + 4)
        .unwrap();
    let limited_pages: i64 = storage
        .connection
        .pragma_query_value(None, "max_page_count", |row| row.get(0))
        .unwrap();
    assert_eq!(limited_pages, pages + 4);

    let batch: Vec<_> = (1..=512).map(|index| event(index, 1.0)).collect();
    let changes_before = storage.connection.total_changes();
    let failure_started = std::time::Instant::now();
    assert_eq!(
        storage.record_metrics(&batch, 2000, 30, 365),
        Err(StorageError::Unavailable)
    );
    let failure_elapsed = failure_started.elapsed();
    // Read immediately: later SQL could overwrite SQLite's underlying error code.
    // SAFETY: Storage owns this live connection; no concurrent access occurs.
    let sqlite_error = unsafe { rusqlite::ffi::sqlite3_errcode(storage.connection.handle()) };
    assert_eq!(sqlite_error, rusqlite::ffi::SQLITE_FULL);
    // SQLite counts successful row mutations even if their transaction rolls back.
    // At least one event insertion and daily update preceded the allocation failure.
    assert!(storage.connection.total_changes() >= changes_before + 2);
    assert!(storage.connection.is_autocommit());
    assert_eq!(
        serde_json::to_value(storage.metric_history(0, 2000).unwrap()).unwrap(),
        prior
    );

    storage
        .connection
        .pragma_update(None, "max_page_count", original_limit)
        .unwrap();
    assert_eq!(storage.record_metrics(&batch, 2000, 30, 365), Ok(512));
    assert_eq!(storage.record_metrics(&batch, 2000, 30, 365), Ok(0));
    drop(storage);
    let mut reopened = Storage::open(&path).unwrap();
    assert_eq!(reopened.record_metrics(&batch, 2000, 30, 365), Ok(0));
    let history = reopened.metric_history(0, 2000).unwrap();
    assert_eq!(history.events.len(), 513);
    assert_eq!(history.daily.len(), 1);
    assert_eq!(history.daily[0].value, 522.0);
    assert_eq!(history.daily[0].count, None);
    assert!(!history.truncated);
    eprintln!(
        "SQLITE_FULL fault injection: page_size={page_size}, limit_pages={limited_pages}, \
         failure_us={}, total_ms={}",
        failure_elapsed.as_micros(),
        started.elapsed().as_millis()
    );
    drop(reopened);
    dir.close().unwrap();
}

fn event(index: u64, value: f64) -> TelemetryDelta {
    let start_ms = index as i64 * 2 + 1;
    let end_ms = start_ms + 1;
    TelemetryDelta {
        provider: Provider::Claude,
        metric: Metric::Tokens,
        dimension: Some("input".into()),
        model: Some("claude-disk-full-fixture".into()),
        start_ms,
        end_ms,
        value,
        count: None,
        gap_before: false,
        series_key: "defaultHasherV1:0123456789abcdef".into(),
        start_ns: (start_ms * 1_000_000).to_string(),
        end_ns: (end_ms * 1_000_000).to_string(),
        counter_start_ns: "1000000".into(),
    }
}
