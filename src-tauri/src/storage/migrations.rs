use super::StorageError;
use rusqlite::Connection;

pub fn apply(connection: &mut Connection) -> Result<(), StorageError> {
    let mut version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|_| StorageError::Corrupt)?;
    if version > 5 {
        return Err(StorageError::Incompatible);
    }
    if version == 0 {
        let transaction = connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        transaction.execute_batch("CREATE TABLE schema_metadata (version INTEGER NOT NULL); INSERT INTO schema_metadata VALUES (1); CREATE TABLE agent_state (agent_id TEXT PRIMARY KEY CHECK(agent_id IN ('claude','codex')), state_json TEXT NOT NULL); PRAGMA user_version=1;").map_err(|_|StorageError::Unavailable)?;
        transaction
            .commit()
            .map_err(|_| StorageError::Unavailable)?;
        version = 1;
    }
    let metadata: i64 = connection
        .query_row("SELECT version FROM schema_metadata", [], |row| row.get(0))
        .map_err(|_| StorageError::Corrupt)?;
    if metadata != version {
        return Err(StorageError::Corrupt);
    }
    if version == 1 {
        let transaction = connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        transaction.execute_batch("UPDATE agent_state SET state_json=json_set(state_json, '$.quota', json('null')); UPDATE schema_metadata SET version=2; PRAGMA user_version=2;")
            .map_err(|_| StorageError::Corrupt)?;
        transaction
            .commit()
            .map_err(|_| StorageError::Unavailable)?;
        version = 2;
    }
    if version == 2 {
        let transaction = connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        transaction.execute_batch("
            CREATE TABLE quota_samples (
                agent_id TEXT NOT NULL,
                bucket_id TEXT NOT NULL,
                observed_ms INTEGER NOT NULL,
                used_percent REAL,
                window_minutes INTEGER,
                resets_at_ms INTEGER,
                source TEXT NOT NULL,
                PRIMARY KEY(agent_id,bucket_id,observed_ms)
            );
            CREATE INDEX quota_samples_observed ON quota_samples(observed_ms);
            CREATE TABLE collection_checks (
                agent_id TEXT NOT NULL,
                observed_ms INTEGER NOT NULL,
                status TEXT NOT NULL,
                diagnostic_code TEXT,
                PRIMARY KEY(agent_id,observed_ms)
            );
            CREATE INDEX collection_checks_observed ON collection_checks(observed_ms);
            CREATE TABLE app_settings (id INTEGER PRIMARY KEY CHECK(id=1), retention_days INTEGER NOT NULL);
            INSERT INTO app_settings VALUES(1,90);
            UPDATE schema_metadata SET version=3;
            PRAGMA user_version=3;
        ").map_err(|_| StorageError::Corrupt)?;
        transaction
            .commit()
            .map_err(|_| StorageError::Unavailable)?;
        version = 3;
    }
    if version == 3 {
        let transaction = connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        transaction.execute_batch("CREATE TABLE preferences (key TEXT PRIMARY KEY CHECK(key IN ('monitor','alerts','alertSeen','telemetry','prices')),value TEXT NOT NULL); CREATE TABLE local_metric_events (event_key TEXT PRIMARY KEY, observed_ms INTEGER NOT NULL, payload TEXT NOT NULL); CREATE INDEX metric_events_time ON local_metric_events(observed_ms); CREATE TABLE daily_metrics (day_ms INTEGER NOT NULL, provider TEXT NOT NULL,metric TEXT NOT NULL,dimension TEXT NOT NULL,model TEXT NOT NULL,value REAL NOT NULL,count INTEGER,PRIMARY KEY(day_ms,provider,metric,dimension,model)); UPDATE schema_metadata SET version=4; PRAGMA user_version=4;").map_err(|_|StorageError::Corrupt)?;
        transaction
            .commit()
            .map_err(|_| StorageError::Unavailable)?;
        version = 4;
    }
    if version == 4 {
        let transaction = connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        // Rebuild only the preference key constraint; preserve every existing row.
        transaction.execute_batch("ALTER TABLE preferences RENAME TO preferences_v4;
            CREATE TABLE preferences(key TEXT PRIMARY KEY CHECK(key IN ('monitor','alerts','alertSeen','telemetry','prices','display')),value TEXT NOT NULL);
            INSERT INTO preferences SELECT key,value FROM preferences_v4;
            DROP TABLE preferences_v4;
            UPDATE schema_metadata SET version=5;
            PRAGMA user_version=5;").map_err(|_| StorageError::Corrupt)?;
        transaction
            .commit()
            .map_err(|_| StorageError::Unavailable)?;
    }
    Ok(())
}
