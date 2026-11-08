use jentrics_core::{
    model::*,
    storage::{Storage, StorageError},
};
use rusqlite::Connection;
use serde_json::json;

fn state(at: i64, percent: Option<f64>, reset: Option<i64>) -> AgentState {
    serde_json::from_value(json!({"agentId":"codex","version":"0.162.1","availability":"detected","authState":"ready","capabilities":{"discovery":true,"authReadiness":true,"quotas":true,"usage":false,"sessionStart":false},"observedMs":at,"diagnosticCode":null,"quota":{"source":"codexAppServer","observedMs":at,"diagnosticCode":null,"windows":[{"bucketId":"codex:primary","windowMinutes":300,"usedPercent":percent,"resetsAtMs":reset,"observedMs":at}]}})).unwrap()
}

#[test]
fn v2_migration_preserves_latest_without_inventing_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE schema_metadata(version INTEGER NOT NULL); INSERT INTO schema_metadata VALUES(2); CREATE TABLE agent_state(agent_id TEXT PRIMARY KEY,state_json TEXT NOT NULL); PRAGMA user_version=2;").unwrap();
    let latest = state(1000, Some(25.0), None);
    db.execute(
        "INSERT INTO agent_state VALUES('codex',?1)",
        [serde_json::to_string(&latest).unwrap()],
    )
    .unwrap();
    drop(db);
    let storage = Storage::open(&path).unwrap();
    assert_eq!(storage.load_agents().unwrap(), vec![latest]);
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        5
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM quota_samples", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM collection_checks", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn samples_are_idempotent_and_unknowns_remain_null() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&dir.path().join("history.sqlite")).unwrap();
    let sample = state(1_000, None, None);
    storage.save_agents(std::slice::from_ref(&sample)).unwrap();
    storage.save_agents(&[sample]).unwrap();
    let view = storage.history(24, 2_000).unwrap();
    assert_eq!(view.samples.len(), 1);
    assert_eq!(view.checks.len(), 1);
    assert_eq!(view.samples[0].used_percent, None);
    assert_eq!(view.samples[0].resets_at_ms, None);
    assert_eq!(view.samples[0].observed_ms, 1_000);
    assert_eq!(view.samples[0].source, "codexAppServer");
    assert_eq!(view.checks[0].status, "reported");
    assert_eq!(view.retention_days, 90);
    assert_eq!(view.from_ms, 0);
    assert_eq!(view.until_ms, 2_000);
    assert!(!view.truncated);
    assert_eq!(view.error, None);
}

#[test]
fn downsampling_preserves_reset_null_and_failed_check_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&dir.path().join("history.sqlite")).unwrap();
    for sample in [
        state(1_000, Some(10.0), Some(500_000)),
        state(2_000, Some(20.0), Some(500_000)),
        state(3_000, Some(0.0), Some(900_000)),
        state(4_000, None, Some(900_000)),
        state(5_000, Some(1.0), Some(900_000)),
    ] {
        storage.save_agents(&[sample]).unwrap();
    }
    storage
        .record_history_failure(
            &[AgentId::Codex, AgentId::Claude],
            3_500,
            DiagnosticCode::ProbeTimeout,
        )
        .unwrap();
    let view = storage.history(24, 6_000).unwrap();
    assert_eq!(
        view.samples
            .iter()
            .map(|s| s.observed_ms)
            .collect::<Vec<_>>(),
        vec![2_000, 4_000, 5_000]
    );
    assert_eq!(view.checks.len(), 3);
    assert!(view.checks.iter().any(|c| c.agent_id == AgentId::Codex
        && c.status == "error"
        && c.diagnostic_code == Some(DiagnosticCode::ProbeTimeout)));
    assert!(view
        .checks
        .iter()
        .any(|c| c.agent_id == AgentId::Claude && c.status == "error"));
    assert_eq!(storage.load_agents().unwrap()[0].observed_ms, 5_000);
}

#[test]
fn retention_prunes_on_save_and_setting_while_clear_preserves_latest() {
    const DAY: i64 = 86_400_000;
    let dir = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&dir.path().join("history.sqlite")).unwrap();
    storage
        .save_agents(&[state(DAY, Some(25.0), None)])
        .unwrap();
    storage
        .save_agents(&[state(8 * DAY, Some(30.0), None)])
        .unwrap();
    assert_eq!(
        storage
            .set_history_retention(7, 9 * DAY)
            .unwrap()
            .retention_days,
        7
    );
    assert_eq!(storage.history(720, 9 * DAY).unwrap().samples.len(), 1);
    storage
        .save_agents(&[state(20 * DAY, Some(50.0), None)])
        .unwrap();
    let db = Connection::open(dir.path().join("history.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM quota_samples", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    storage.clear_history().unwrap();
    assert!(storage.history(720, 20 * DAY).unwrap().samples.is_empty());
    assert!(storage.history(720, 20 * DAY).unwrap().checks.is_empty());
    assert_eq!(storage.load_agents().unwrap()[0].observed_ms, 20 * DAY);
    assert_eq!(storage.history_settings().unwrap().retention_days, 7);
    storage
        .save_agents(&[state(20 * DAY + 1, Some(55.0), None)])
        .unwrap();
    assert_eq!(storage.history(24, 20 * DAY + 2).unwrap().samples.len(), 1);
}

#[test]
fn invalid_inputs_and_batches_fail_without_partial_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&dir.path().join("history.sqlite")).unwrap();
    storage
        .save_agents(&[state(1000, Some(5.0), None)])
        .unwrap();
    let mut invalid = state(2000, Some(10.0), None);
    invalid.quota.as_mut().unwrap().windows[0].used_percent = Some(101.0);
    assert_eq!(
        storage.save_agents(&[state(1500, Some(8.0), None), invalid]),
        Err(StorageError::InvalidState)
    );
    assert_eq!(
        storage.history(24, 3000).unwrap().samples[0].observed_ms,
        1000
    );
    assert_eq!(storage.load_agents().unwrap()[0].observed_ms, 1000);
    for range in [0, 1, 25, 721] {
        assert_eq!(
            storage.history(range, 3000),
            Err(StorageError::InvalidState)
        );
    }
    for days in [0, 1, 8, 366] {
        assert_eq!(
            storage.set_history_retention(days, 3000),
            Err(StorageError::InvalidState)
        );
    }
    for now in [-1, i64::MAX] {
        assert_eq!(storage.history(24, now), Err(StorageError::InvalidState));
        assert_eq!(
            storage.set_history_retention(7, now),
            Err(StorageError::InvalidState)
        );
        assert_eq!(
            storage.record_history_failure(&[AgentId::Codex], now, DiagnosticCode::ProbeTimeout),
            Err(StorageError::InvalidState)
        );
    }
}

#[test]
fn sql_write_failure_rolls_back_latest_samples_and_checks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let mut storage = Storage::open(&path).unwrap();
    storage
        .save_agents(&[state(1000, Some(5.0), None)])
        .unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_history BEFORE INSERT ON collection_checks WHEN NEW.observed_ms=2000 BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert_eq!(
        storage.save_agents(&[state(2000, Some(10.0), None)]),
        Err(StorageError::Unavailable)
    );
    assert_eq!(storage.load_agents().unwrap()[0].observed_ms, 1000);
    assert_eq!(
        storage.history(24, 3000).unwrap().samples[0].observed_ms,
        1000
    );
}

#[test]
fn history_query_bounds_rows_and_reports_corrupt_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let storage = Storage::open(&path).unwrap();
    let mut db = Connection::open(&path).unwrap();
    let tx = db.transaction().unwrap();
    for i in 0..10_002_i64 {
        tx.execute("INSERT INTO quota_samples(agent_id,bucket_id,observed_ms,used_percent,window_minutes,resets_at_ms,source) VALUES('codex','codex:primary',?1,NULL,NULL,?1,'codexAppServer')",[i]).unwrap();
        tx.execute("INSERT INTO collection_checks(agent_id,observed_ms,status,diagnostic_code) VALUES('codex',?1,'error',NULL)",[i*300_000]).unwrap();
    }
    tx.commit().unwrap();
    let view = storage.history(720, 10_002).unwrap();
    assert_eq!(view.samples.len(), 10_000);
    assert!(view.truncated);
    db.execute(
        "UPDATE quota_samples SET source='privateUnknown' WHERE observed_ms=10001",
        [],
    )
    .unwrap();
    assert_eq!(storage.history(24, 10_002), Err(StorageError::Corrupt));
}

#[test]
fn range_bins_are_five_minutes_one_hour_and_six_hours() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&dir.path().join("history.sqlite")).unwrap();
    for at in [1_000, 310_000, 3_610_000, 21_610_000] {
        storage.save_agents(&[state(at, Some(10.0), None)]).unwrap();
    }
    assert_eq!(
        storage
            .history(24, 22_000_000)
            .unwrap()
            .samples
            .iter()
            .map(|s| s.observed_ms)
            .collect::<Vec<_>>(),
        vec![1_000, 310_000, 3_610_000, 21_610_000]
    );
    assert_eq!(
        storage
            .history(168, 22_000_000)
            .unwrap()
            .samples
            .iter()
            .map(|s| s.observed_ms)
            .collect::<Vec<_>>(),
        vec![310_000, 3_610_000, 21_610_000]
    );
    assert_eq!(
        storage
            .history(720, 22_000_000)
            .unwrap()
            .samples
            .iter()
            .map(|s| s.observed_ms)
            .collect::<Vec<_>>(),
        vec![3_610_000, 21_610_000]
    );
    assert!(storage.history(24, 120_000_000).unwrap().samples.is_empty());
    assert!(storage.history(24, 500).unwrap().samples.is_empty());
}

#[test]
fn checks_keep_latest_per_provider_bin_status_and_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&dir.path().join("history.sqlite")).unwrap();
    let mut unavailable = state(1_000, None, None);
    unavailable.quota = None;
    unavailable.capabilities.quotas = false;
    storage.save_agents(&[unavailable]).unwrap();
    for (at, code) in [
        (2_000, DiagnosticCode::ProbeTimeout),
        (3_000, DiagnosticCode::ProbeTimeout),
        (4_000, DiagnosticCode::QuotaUnavailable),
    ] {
        storage
            .record_history_failure(&[AgentId::Codex], at, code)
            .unwrap();
    }
    storage
        .save_agents(&[state(5_000, Some(1.0), None)])
        .unwrap();
    storage
        .record_history_failure(&[AgentId::Claude], 6_000, DiagnosticCode::ProbeTimeout)
        .unwrap();
    let checks = storage.history(24, 7_000).unwrap().checks;
    assert_eq!(checks.len(), 5);
    assert!(!checks.iter().any(|c| c.observed_ms == 2_000));
    assert!(checks
        .iter()
        .any(|c| c.observed_ms == 1_000 && c.status == "unavailable"));
    assert!(
        checks
            .iter()
            .any(|c| c.observed_ms == 3_000
                && c.diagnostic_code == Some(DiagnosticCode::ProbeTimeout))
    );
    assert!(checks
        .iter()
        .any(|c| c.observed_ms == 4_000
            && c.diagnostic_code == Some(DiagnosticCode::QuotaUnavailable)));
}

#[test]
fn corrupt_settings_and_rows_are_errors_not_empty_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let mut storage = Storage::open(&path).unwrap();
    storage
        .save_agents(&[state(1_000, Some(1.0), None)])
        .unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute("UPDATE app_settings SET retention_days=8", [])
        .unwrap();
    assert_eq!(storage.history_settings(), Err(StorageError::Corrupt));
    assert_eq!(storage.history(24, 2_000), Err(StorageError::Corrupt));
    assert_eq!(
        storage.set_history_retention(7, 2_000),
        Err(StorageError::Corrupt)
    );
    assert_eq!(
        storage.save_agents(&[state(2_000, Some(2.0), None)]),
        Err(StorageError::Corrupt)
    );
    assert_eq!(storage.load_agents().unwrap()[0].observed_ms, 1_000);
    db.execute("UPDATE app_settings SET retention_days=90", [])
        .unwrap();
    db.execute("UPDATE collection_checks SET status='invalid'", [])
        .unwrap();
    assert_eq!(storage.history(24, 2_000), Err(StorageError::Corrupt));
    db.execute(
        "UPDATE collection_checks SET status='reported',diagnostic_code='unknownCode'",
        [],
    )
    .unwrap();
    assert_eq!(storage.history(24, 2_000), Err(StorageError::Corrupt));
}

#[test]
fn failed_v3_migration_does_not_partially_change_v2() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE schema_metadata(version INTEGER NOT NULL); INSERT INTO schema_metadata VALUES(2); CREATE TABLE agent_state(agent_id TEXT PRIMARY KEY,state_json TEXT NOT NULL); CREATE TABLE quota_samples(preserve TEXT); INSERT INTO quota_samples VALUES('preserved'); PRAGMA user_version=2;").unwrap();
    assert!(matches!(Storage::open(&path), Err(StorageError::Corrupt)));
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row("SELECT preserve FROM quota_samples", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "preserved"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='collection_checks'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
