use super::{MaintenanceError, MaintenanceOutcome, Storage};
use crate::{
    model::AgentState,
    storage::PreferenceKey,
    telemetry::{Metric, Provider, TelemetryDelta},
};

fn populate(storage: &mut Storage) {
    let events: Vec<_> = (1..=6000)
        .map(|i| TelemetryDelta {
            provider: Provider::Claude,
            metric: Metric::Tokens,
            dimension: Some("input".into()),
            model: Some("maintenance-fixture".into()),
            start_ms: i * 2,
            end_ms: i * 2 + 1,
            value: 1.0,
            count: None,
            gap_before: false,
            series_key: "defaultHasherV1:0123456789abcdef".into(),
            start_ns: (i * 2 * 1_000_000).to_string(),
            end_ns: ((i * 2 + 1) * 1_000_000).to_string(),
            counter_start_ns: "1000000".into(),
        })
        .collect();
    for chunk in events.chunks(2000) {
        assert_eq!(storage.record_metrics(chunk, 20000, 30, 365).unwrap(), 2000);
    }
    let state: AgentState = serde_json::from_value(serde_json::json!({"agentId":"codex","version":null,"availability":"notFound","authState":"unknown","capabilities":{"discovery":true,"authReadiness":false,"quotas":false,"usage":false,"sessionStart":false},"observedMs":20000,"diagnosticCode":"executableUnavailable","quota":null})).unwrap();
    storage.save_agents(&[state]).unwrap();
    storage
        .write_preference(
            PreferenceKey::Monitor,
            &serde_json::json!({"paused":true,"intervalMinutes":15}),
        )
        .unwrap();
    storage
        .connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
}

#[test]
fn clear_reclaims_real_pages_and_preserves_latest_preferences_and_quota_checks() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("maintenance.sqlite");
    let mut storage = Storage::open(&path).unwrap();
    populate(&mut storage);
    let before = std::fs::metadata(&path).unwrap().len();
    let latest = storage.load_agents().unwrap();
    let preference = storage.read_preference(PreferenceKey::Monitor).unwrap();
    let checks = storage.history(24, 20000).unwrap().checks;
    storage.clear_metrics().unwrap();
    let status = storage.maintenance_status();
    assert_eq!(status.outcome, MaintenanceOutcome::Compacted);
    assert!(status.data_committed);
    assert_eq!(status.error, None);
    assert!(status.reclaimed_bytes.unwrap() >= 1024 * 1024);
    assert!(std::fs::metadata(&path).unwrap().len() < before);
    assert!(storage.metric_history(0, 20000).unwrap().events.is_empty());
    assert_eq!(storage.load_agents().unwrap(), latest);
    assert_eq!(
        storage.read_preference(PreferenceKey::Monitor).unwrap(),
        preference
    );
    assert_eq!(storage.history(24, 20000).unwrap().checks, checks);
}

#[test]
fn retention_reclaims_event_pages_without_removing_retained_daily_totals() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("retention.sqlite");
    let mut storage = Storage::open(&path).unwrap();
    populate(&mut storage);
    let before = std::fs::metadata(&path).unwrap().len();
    storage.record_metrics(&[], 31 * 86400000, 30, 365).unwrap();
    assert_eq!(
        storage.maintenance_status().outcome,
        MaintenanceOutcome::Compacted
    );
    let history = storage.metric_history(0, 32 * 86400000).unwrap();
    assert!(history.events.is_empty());
    assert_eq!(history.daily.len(), 1);
    assert_eq!(history.daily[0].value, 6000.0);
    assert!(std::fs::metadata(&path).unwrap().len() < before);
    assert_eq!(
        storage.read_preference(PreferenceKey::Monitor).unwrap(),
        Some(serde_json::json!({"paused":true,"intervalMinutes":15}))
    );
    assert_eq!(storage.load_agents().unwrap().len(), 1);
}

#[test]
fn busy_compaction_is_visible_after_committed_clear_and_can_be_retried() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("busy.sqlite");
    let mut storage = Storage::open(&path).unwrap();
    populate(&mut storage);
    storage
        .connection
        .busy_timeout(std::time::Duration::from_millis(25))
        .unwrap();
    let reader = rusqlite::Connection::open(path).unwrap();
    reader.execute_batch("BEGIN;").unwrap();
    assert_eq!(
        reader
            .query_row("SELECT COUNT(*) FROM local_metric_events", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        6000
    );
    assert_eq!(storage.clear_metrics(), Ok(()));
    let status = storage.maintenance_status();
    assert!(status.data_committed);
    assert_eq!(status.outcome, MaintenanceOutcome::Failed);
    assert_eq!(status.error, Some(MaintenanceError::Busy));
    assert!(storage.metric_history(0, 20000).unwrap().events.is_empty());
    reader.execute_batch("ROLLBACK;").unwrap();
    storage.clear_metrics().unwrap();
    assert_eq!(
        storage.maintenance_status().outcome,
        MaintenanceOutcome::Compacted
    );
    assert!(storage.metric_history(0, 20000).unwrap().daily.is_empty());
}
