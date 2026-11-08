use jentrics_core::{
    configuration::{AppConfiguration, ConfigurationError, DisplaySettings},
    model::{AgentState, DiagnosticCode},
    state::{AppState, Collector},
    storage::{PreferenceKey, Storage, StorageError},
};
use serde_json::json;
use std::{future::Future, pin::Pin, sync::Arc};

struct NoProvider;
impl Collector for NoProvider {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async { panic!("configuration must not invoke providers") })
    }
}
fn state(path: &std::path::Path) -> Arc<AppState> {
    AppState::new(Storage::open(path), Arc::new(NoProvider))
}

#[test]
fn export_import_roundtrip_replaces_only_app_settings_and_keeps_unknown_selection() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("config.sqlite");
    let app = state(&path);
    let initial = app.export_configuration().unwrap();
    assert_eq!(initial.schema_version, 1);
    assert_eq!(initial.quota_retention_days, 90);
    assert!(!initial.alerts.enabled);
    assert!(!initial.telemetry.enabled);
    assert_eq!(initial.display.primary_buckets.codex, None);
    let mut replacement = initial;
    replacement.monitoring.paused = true;
    replacement.monitoring.interval_minutes = 15;
    replacement.quota_retention_days = 30;
    replacement.display.primary_buckets.codex = Some("missing:primary".into());
    assert_eq!(
        app.import_configuration(replacement.clone(), true).unwrap(),
        replacement
    );
    assert_eq!(app.display_settings().unwrap(), replacement.display);
    drop(app);
    assert_eq!(state(&path).export_configuration().unwrap(), replacement);
}

#[test]
fn cancellation_validation_failure_and_sql_failure_never_partially_apply() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("config.sqlite");
    let app = state(&path);
    let original = app.export_configuration().unwrap();
    let mut changed = original.clone();
    changed.monitoring.paused = true;
    changed.quota_retention_days = 7;
    assert_eq!(
        app.import_configuration(changed.clone(), false),
        Err(ConfigurationError::ConfirmationRequired)
    );
    assert_eq!(app.export_configuration().unwrap(), original);
    let mut invalid = changed.clone();
    invalid.telemetry.event_retention_days = 90;
    invalid.telemetry.daily_retention_days = 30;
    assert_eq!(
        app.import_configuration(invalid, true),
        Err(ConfigurationError::InvalidConfiguration)
    );
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_import BEFORE INSERT ON preferences WHEN NEW.key='telemetry' BEGIN SELECT RAISE(ABORT,'fixture rejection'); END;").unwrap();
    assert_eq!(
        app.import_configuration(changed, true),
        Err(ConfigurationError::StorageUnavailable)
    );
    assert_eq!(app.export_configuration().unwrap(), original);
    assert!(!app.metrics_view(1).unwrap().running);
}

#[test]
fn late_retention_failure_rolls_back_prior_pruning_and_every_setting() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("late-failure.sqlite");
    let app = state(&path);
    let original = app.export_configuration().unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch("INSERT INTO quota_samples VALUES('codex','codex:primary',1,10,300,NULL,'codexAppServer');
        INSERT INTO collection_checks VALUES('codex',1,'reported',NULL);
        INSERT INTO daily_metrics VALUES(0,'claude','tokens','input','fixture',10,NULL);
        CREATE TRIGGER reject_daily_prune BEFORE DELETE ON daily_metrics BEGIN SELECT RAISE(ABORT,'late fixture rejection'); END;").unwrap();
    let mut changed = original.clone();
    changed.quota_retention_days = 7;
    changed.monitoring.paused = true;
    assert_eq!(
        app.import_configuration(changed, true),
        Err(ConfigurationError::StorageUnavailable)
    );
    assert_eq!(app.export_configuration().unwrap(), original);
    for table in ["quota_samples", "collection_checks", "daily_metrics"] {
        assert_eq!(
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}

#[test]
fn raw_configuration_rejects_unknown_nested_future_and_oversized_documents() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let app = state(&dir.path().join("config.sqlite"));
    let encoded = serde_json::to_value(app.export_configuration().unwrap()).unwrap();
    for path in ["root", "monitoring", "display", "primaryBuckets"] {
        let mut bad = encoded.clone();
        let target = match path {
            "root" => &mut bad,
            "primaryBuckets" => &mut bad["display"]["primaryBuckets"],
            _ => &mut bad[path],
        };
        target["secretPath"] = json!("SYNTHETIC_PRIVATE");
        assert_eq!(
            AppConfiguration::from_json(&bad.to_string()),
            Err(ConfigurationError::InvalidConfiguration)
        );
    }
    let mut future = encoded;
    future["schemaVersion"] = json!(2);
    assert_eq!(
        AppConfiguration::from_json(&future.to_string()),
        Err(ConfigurationError::UnsupportedSchema)
    );
    assert_eq!(
        AppConfiguration::from_json("{"),
        Err(ConfigurationError::InvalidConfiguration)
    );
    assert_eq!(
        AppConfiguration::from_json(&" ".repeat(128 * 1024 + 1)),
        Err(ConfigurationError::TooLarge)
    );
    let valid = app.export_configuration().unwrap();
    let mut exact = serde_json::to_string(&valid).unwrap();
    exact.push_str(&" ".repeat(128 * 1024 - exact.len()));
    assert_eq!(AppConfiguration::from_json(&exact), Ok(valid));
}

#[test]
fn display_requires_explicit_nullable_fields_and_valid_bounded_ids() {
    assert!(
        serde_json::from_value::<DisplaySettings>(json!({"primaryBuckets":{"claude":null}}))
            .is_err()
    );
    assert!(serde_json::from_value::<DisplaySettings>(
        json!({"primaryBuckets":{"claude":null,"codex":null,"gemini":null}})
    )
    .is_err());
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let app = state(&dir.path().join("config.sqlite"));
    let original = app.display_settings().unwrap();
    for id in [
        "".to_string(),
        "a".repeat(139),
        "bad\nprovider".to_string(),
        "bad\u{0085}provider".to_string(),
        "界".repeat(47),
    ] {
        let mut bad = original.clone();
        bad.primary_buckets.claude = Some(id);
        assert_eq!(
            app.save_display_settings(bad),
            Err(ConfigurationError::InvalidConfiguration)
        );
    }
    let mut valid = original;
    valid.primary_buckets.claude = Some("a".repeat(138));
    valid.primary_buckets.codex = Some("codex:primary".into());
    assert_eq!(app.save_display_settings(valid.clone()).unwrap(), valid);
    assert_eq!(
        app.save_display_settings(DisplaySettings::default())
            .unwrap(),
        DisplaySettings::default()
    );
}

#[test]
fn reported_unicode_slash_bucket_selection_roundtrips_without_rewriting() {
    let windows = jentrics_core::adapters::codex::normalize_codex_limits(
        &json!({"rateLimits":{"limitId":"custom quota/配额","primary":{"usedPercent":10}}}),
        1000,
    )
    .unwrap();
    assert_eq!(windows[0].bucket_id, "custom quota/配额:primary");
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("unicode-display.sqlite");
    let app = state(&path);
    let mut settings = app.display_settings().unwrap();
    settings.primary_buckets.codex = Some(windows[0].bucket_id.clone());
    settings.primary_buckets.claude = Some("界".repeat(46));
    assert_eq!(
        app.save_display_settings(settings.clone()),
        Ok(settings.clone())
    );
    assert_eq!(app.export_configuration().unwrap().display, settings);
    drop(app);
    assert_eq!(state(&path).display_settings().unwrap(), settings);
}

#[test]
fn duplicate_prices_invalid_settings_and_preference_limit_fail_before_mutation() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let app = state(&dir.path().join("config.sqlite"));
    let original = app.export_configuration().unwrap();
    let price = jentrics_core::pricing::PriceRegistry::bundled()
        .unwrap()
        .prices()
        .remove(0);
    let mut duplicate = original.clone();
    duplicate.price_overrides = vec![price.clone(), price.clone()];
    assert_eq!(
        app.import_configuration(duplicate, true),
        Err(ConfigurationError::InvalidConfiguration)
    );
    let mut oversized = original.clone();
    for index in 0..80 {
        let mut item = price.clone();
        item.model_id = format!("fixture-{index}");
        item.source_urls = vec![format!("https://example.invalid/{}", "a".repeat(470))];
        oversized.price_overrides.push(item);
    }
    assert!(
        serde_json::to_vec(&oversized.price_overrides)
            .unwrap()
            .len()
            > 64 * 1024
    );
    assert!(serde_json::to_vec(&oversized).unwrap().len() < 128 * 1024);
    assert_eq!(
        app.import_configuration(oversized, true),
        Err(ConfigurationError::TooLarge)
    );
    for minutes in [0, 1, 6, 60] {
        let mut bad = original.clone();
        bad.monitoring.interval_minutes = minutes;
        assert_eq!(
            app.import_configuration(bad, true),
            Err(ConfigurationError::InvalidConfiguration)
        );
    }
    assert_eq!(app.export_configuration().unwrap(), original);
}

#[test]
fn corrupt_saved_configuration_is_an_error_and_manual_prices_keep_unknown_classes() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("corrupt.sqlite");
    let app = state(&path);
    let mut configuration = app.export_configuration().unwrap();
    let mut price = jentrics_core::pricing::PriceRegistry::bundled()
        .unwrap()
        .prices()
        .remove(0);
    price.short_context.cache_read_usd_per_million = None;
    price.long_context.cache_read_usd_per_million = None;
    configuration.price_overrides.push(price);
    assert_eq!(
        app.import_configuration(configuration.clone(), true)
            .unwrap(),
        configuration
    );
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute("UPDATE preferences SET value='{}' WHERE key='monitor'", [])
        .unwrap();
    assert_eq!(
        app.export_configuration(),
        Err(ConfigurationError::StorageCorrupt)
    );
    assert_eq!(
        app.import_configuration(configuration.clone(), true),
        Ok(configuration)
    );
}

#[test]
fn failed_schema5_upgrade_preserves_original_schema4_preferences() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("failed-upgrade.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA user_version=4; CREATE TABLE schema_metadata(version INTEGER); INSERT INTO schema_metadata VALUES(4);
        CREATE TABLE preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO preferences VALUES('unexpected','preserve-corrupt-row');").unwrap();
    drop(db);
    assert!(matches!(Storage::open(&path), Err(StorageError::Corrupt)));
    let db = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        4
    );
    assert_eq!(
        db.query_row(
            "SELECT value FROM preferences WHERE key='unexpected'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "preserve-corrupt-row"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='preferences_v4'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn export_excludes_observations_dedup_and_other_private_state() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("config.sqlite");
    let mut storage = Storage::open(&path).unwrap();
    storage
        .write_preference(
            PreferenceKey::AlertSeen,
            &json!(["SYNTHETIC_PRIVATE_DEDUP"]),
        )
        .unwrap();
    let app = AppState::new(Ok(storage), Arc::new(NoProvider));
    let export = serde_json::to_string(&app.export_configuration().unwrap()).unwrap();
    for forbidden in [
        "SYNTHETIC_PRIVATE",
        "observedMs",
        "agentState",
        "accountUsage",
        "alertSeen",
        "bridge",
        "executable",
    ] {
        assert!(!export.contains(forbidden), "unexpected field {forbidden}");
    }
    let imported = app.export_configuration().unwrap();
    app.import_configuration(imported, true).unwrap();
    let db = Storage::open(&path).unwrap();
    assert_eq!(
        db.read_preference(PreferenceKey::AlertSeen).unwrap(),
        Some(json!(["SYNTHETIC_PRIVATE_DEDUP"]))
    );
}

#[test]
fn enabling_alerts_without_permission_reports_runtime_failure_after_atomic_commit() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let app = state(&dir.path().join("config.sqlite"));
    let mut config = app.export_configuration().unwrap();
    config.alerts.enabled = true;
    config.monitoring.paused = true;
    assert_eq!(
        app.import_configuration(config.clone(), true),
        Err(ConfigurationError::RuntimeUnavailable)
    );
    assert_eq!(app.export_configuration().unwrap(), config);
    assert!(!app.metrics_view(1).unwrap().running);
}

#[test]
fn occupied_listener_reports_committed_activation_failure_and_disabled_import_recovers() {
    let occupied = std::net::TcpListener::bind("127.0.0.1:43190").unwrap();
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let app = state(&dir.path().join("config.sqlite"));
    let mut config = app.export_configuration().unwrap();
    config.telemetry.enabled = true;
    assert_eq!(
        app.import_configuration(config.clone(), true),
        Err(ConfigurationError::RuntimeUnavailable)
    );
    assert_eq!(app.export_configuration().unwrap(), config);
    assert!(!app.metrics_view(1).unwrap().running);
    config.telemetry.enabled = false;
    assert_eq!(app.import_configuration(config.clone(), true), Ok(config));
    assert_eq!(app.metrics_view(1).unwrap().status.error, None);
    drop(occupied);
}

#[test]
fn schema4_migration_preserves_all_existing_tables_preferences_and_future6_is_refused() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("v4.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE schema_metadata(version INTEGER NOT NULL); INSERT INTO schema_metadata VALUES(4); PRAGMA user_version=4;
        CREATE TABLE agent_state(agent_id TEXT PRIMARY KEY,state_json TEXT NOT NULL); INSERT INTO agent_state VALUES('codex','preserve-latest');
        CREATE TABLE quota_samples(marker TEXT); INSERT INTO quota_samples VALUES('preserve-quota');
        CREATE TABLE collection_checks(marker TEXT); INSERT INTO collection_checks VALUES('preserve-check');
        CREATE TABLE local_metric_events(marker TEXT); INSERT INTO local_metric_events VALUES('preserve-events');
        CREATE TABLE daily_metrics(marker TEXT); INSERT INTO daily_metrics VALUES('preserve-daily');
        CREATE TABLE app_settings(id INTEGER PRIMARY KEY,retention_days INTEGER); INSERT INTO app_settings VALUES(1,180);
        CREATE TABLE preferences(key TEXT PRIMARY KEY CHECK(key IN ('monitor','alerts','alertSeen','telemetry','prices')),value TEXT NOT NULL);
        INSERT INTO preferences VALUES('alertSeen','[\"preserve-dedup\"]');").unwrap();
    for (key, value) in [
        ("monitor", json!({"paused":true,"intervalMinutes":15})),
        (
            "alerts",
            serde_json::to_value(jentrics_core::alerts::AlertSettings::default()).unwrap(),
        ),
        (
            "telemetry",
            serde_json::to_value(jentrics_core::metrics::MetricSettings::default()).unwrap(),
        ),
        ("prices", json!([])),
    ] {
        db.execute(
            "INSERT INTO preferences VALUES(?1,?2)",
            rusqlite::params![key, value.to_string()],
        )
        .unwrap();
    }
    drop(db);
    let storage = Storage::open(&path).unwrap();
    assert_eq!(storage.history_settings().unwrap().retention_days, 180);
    assert_eq!(
        storage.read_preference(PreferenceKey::AlertSeen).unwrap(),
        Some(json!(["preserve-dedup"]))
    );
    assert_eq!(
        storage.read_preference(PreferenceKey::Monitor).unwrap(),
        Some(json!({"paused":true,"intervalMinutes":15}))
    );
    assert_eq!(
        storage.read_preference(PreferenceKey::Prices).unwrap(),
        Some(json!([]))
    );
    drop(storage);
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        5
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM preferences", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        5
    );
    for (table, column, expected) in [
        ("agent_state", "state_json", "preserve-latest"),
        ("quota_samples", "marker", "preserve-quota"),
        ("collection_checks", "marker", "preserve-check"),
        ("local_metric_events", "marker", "preserve-events"),
        ("daily_metrics", "marker", "preserve-daily"),
    ] {
        assert_eq!(
            db.query_row(&format!("SELECT {column} FROM {table}"), [], |row| row
                .get::<_, String>(
                0
            ))
            .unwrap(),
            expected
        );
    }
    db.execute_batch("PRAGMA user_version=6;").unwrap();
    drop(db);
    assert!(matches!(
        Storage::open(&path),
        Err(StorageError::Incompatible)
    ));
    let db = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        db.query_row("SELECT state_json FROM agent_state", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "preserve-latest"
    );
}
