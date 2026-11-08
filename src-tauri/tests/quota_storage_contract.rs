use jentrics_core::{
    model::*,
    storage::{Storage, StorageError},
};
use serde_json::json;

fn state() -> AgentState {
    serde_json::from_value(json!({"agentId":"codex","version":"0.162.1","availability":"detected","authState":"ready","capabilities":{"discovery":true,"authReadiness":true,"quotas":true,"usage":false,"sessionStart":false},"observedMs":1000,"diagnosticCode":null,
    "quota":{"source":"codexAppServer","observedMs":1000,"diagnosticCode":null,"windows":[{"bucketId":"codex:primary","windowMinutes":300,"usedPercent":25,"resetsAtMs":1800000000000_i64,"observedMs":1000}]}})).unwrap()
}
#[test]
fn quota_latest_snapshot_round_trips_without_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let mut storage = Storage::open(&path).unwrap();
    storage.save_agents(&[state()]).unwrap();
    drop(storage);
    assert_eq!(
        Storage::open(&path).unwrap().load_agents().unwrap(),
        vec![state()]
    );
}
#[test]
fn legacy_readiness_rows_migrate_explicitly_to_unchecked_quota() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let mut legacy = serde_json::to_value(state()).unwrap();
    legacy.as_object_mut().unwrap().remove("quota");
    legacy["capabilities"]["quotas"] = json!(false);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE schema_metadata (version INTEGER NOT NULL); INSERT INTO schema_metadata VALUES (1); CREATE TABLE agent_state (agent_id TEXT PRIMARY KEY, state_json TEXT NOT NULL); PRAGMA user_version=1;").unwrap();
    connection
        .execute(
            "INSERT INTO agent_state VALUES ('codex',?1)",
            [legacy.to_string()],
        )
        .unwrap();
    drop(connection);
    let migrated = Storage::open(&path).unwrap().load_agents().unwrap();
    assert_eq!(migrated[0].auth_state, AuthState::Ready);
    assert!(serde_json::to_value(&migrated[0])
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("quota"));
    assert!(migrated[0].quota.is_none());
    let connection = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        5
    );
}
#[test]
fn invalid_quota_cannot_be_saved_as_valid_data() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&dir.path().join("state.sqlite")).unwrap();
    for mutation in 0..7 {
        let mut value = state();
        let quota = value.quota.as_mut().unwrap();
        match mutation {
            0 => quota.windows[0].used_percent = Some(f64::NAN),
            1 => quota.windows[0].resets_at_ms = Some(i64::MAX),
            2 => quota.windows[0].window_minutes = Some(0),
            3 => quota.windows[0].bucket_id = "x".repeat(200),
            4 => quota.windows = vec![quota.windows[0].clone(); 33],
            5 => quota.diagnostic_code = Some(DiagnosticCode::QuotaUnavailable),
            _ => value.agent_id = AgentId::Claude,
        }
        assert_eq!(
            storage.save_agents(&[value]),
            Err(StorageError::InvalidState)
        );
    }
}
