use jentrics_core::diagnostics::{Diagnostics, EventCode, StatusCode};

#[test]
fn fixed_json_is_flushed_on_shutdown_and_guard_lives_between_events() {
    let dir = tempfile::tempdir().unwrap();
    let diagnostics = Diagnostics::open(dir.path());
    assert_eq!(diagnostics.status().code, StatusCode::Healthy);
    diagnostics.record(EventCode::Startup, 12, 2);
    diagnostics.record(EventCode::RefreshCompleted, 20, 1);
    diagnostics.shutdown();
    assert_eq!(diagnostics.status().code, StatusCode::Stopped);
    assert_eq!(diagnostics.status().written_records, 2);
    let logs = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<String>();
    let records: Vec<serde_json::Value> = logs
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["code"], "startup");
    assert_eq!(records[0]["durationMs"], 12);
    assert!(records
        .iter()
        .all(|record| record.as_object().unwrap().len() == 5));
    diagnostics.record(EventCode::Startup, 1, 1);
    assert_eq!(diagnostics.status().written_records, 2);
}

#[test]
fn initialization_failure_is_fixed_and_contains_no_path_or_raw_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("private-config-name");
    std::fs::write(&file, "private payload").unwrap();
    let diagnostics = Diagnostics::open(&file);
    let status = serde_json::to_string(&diagnostics.status()).unwrap();
    assert_eq!(
        diagnostics.status().code,
        StatusCode::InitializationUnavailable
    );
    assert!(!status.contains("private"));
    assert!(!status.contains("path"));
}

#[test]
fn existing_logs_are_retained_in_bounded_daily_set() {
    let dir = tempfile::tempdir().unwrap();
    for day in 1..=12 {
        std::fs::write(dir.path().join(format!("jentrics.{day}.jsonl")), "{}\n").unwrap();
    }
    let diagnostics = Diagnostics::open(dir.path());
    diagnostics.record(EventCode::Startup, 0, 0);
    diagnostics.shutdown();
    assert!(std::fs::read_dir(dir.path()).unwrap().count() <= 7);
}

#[test]
fn oversized_existing_logs_stop_growth_with_explicit_capacity_status() {
    let dir = tempfile::tempdir().unwrap();
    let diagnostics = Diagnostics::open(dir.path());
    let path = std::fs::read_dir(dir.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(8 * 1024 * 1024)
        .unwrap();
    diagnostics.record(EventCode::Startup, 0, 0);
    diagnostics.shutdown();
    assert_eq!(diagnostics.status().code, StatusCode::CapacityReached);
    assert_eq!(std::fs::metadata(path).unwrap().len(), 8 * 1024 * 1024);
}
