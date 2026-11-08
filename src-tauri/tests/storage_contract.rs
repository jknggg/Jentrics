use jentrics_core::{
    model::*,
    storage::{Storage, StorageError},
};

fn sample() -> AgentState {
    AgentState {
        agent_id: AgentId::Codex,
        version: Some("0.162.0-alpha.17.2".into()),
        availability: Availability::Detected,
        auth_state: AuthState::Ready,
        capabilities: Capabilities {
            discovery: true,
            auth_readiness: true,
            ..Default::default()
        },
        observed_ms: 1000,
        diagnostic_code: None,
        quota: None,
        account_usage: None,
    }
}
#[test]
fn migration_is_idempotent_and_samples_upsert() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.sqlite");
    let mut storage = Storage::open(&db).unwrap();
    storage.save_agents(&[sample()]).unwrap();
    let mut updated = sample();
    updated.observed_ms = 2000;
    storage.save_agents(std::slice::from_ref(&updated)).unwrap();
    drop(storage);
    assert_eq!(
        Storage::open(&db).unwrap().load_agents().unwrap(),
        vec![updated]
    );
    let connection = rusqlite::Connection::open(db).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM agent_state", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn future_schema_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.sqlite");
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection.execute_batch("PRAGMA user_version=99; CREATE TABLE future_data (value TEXT); INSERT INTO future_data VALUES ('preserve');").unwrap();
    drop(connection);
    assert!(matches!(
        Storage::open(&db),
        Err(StorageError::Incompatible)
    ));
    assert_eq!(
        rusqlite::Connection::open(db)
            .unwrap()
            .query_row("SELECT value FROM future_data", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "preserve"
    );
}
#[test]
fn failed_batch_rolls_back_previous_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.sqlite");
    let mut storage = Storage::open(&db).unwrap();
    storage.save_agents(&[sample()]).unwrap();
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_claude BEFORE INSERT ON agent_state WHEN NEW.agent_id='claude' BEGIN SELECT RAISE(ABORT,'test rejection'); END;").unwrap();
    let mut updated = sample();
    updated.observed_ms = 2000;
    let mut claude = sample();
    claude.agent_id = AgentId::Claude;
    assert_eq!(
        storage.save_agents(&[updated, claude]),
        Err(StorageError::Unavailable)
    );
    assert_eq!(storage.load_agents().unwrap(), vec![sample()]);
}
#[test]
fn state_serialization_has_only_safe_public_metadata() {
    let encoded = serde_json::to_string(&sample()).unwrap();
    for forbidden in ["executable", "email", "stdout", "stderr", "token", "path"] {
        assert!(!encoded.contains(forbidden));
    }
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.sqlite");
    let mut state = sample();
    state.version = Some("private/path".into());
    assert_eq!(
        Storage::open(&db).unwrap().save_agents(&[state]),
        Err(StorageError::InvalidState)
    );
}
