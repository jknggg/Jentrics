use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn developer_probe_fails_when_search_path_is_missing() {
    let cwd = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_provider_probe"));
    command
        .env_remove("PATH")
        .current_dir(cwd.path())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .expect("probe did not terminate")
        .unwrap();
    assert!(
        !output.status.success(),
        "missing PATH must fail, not return success"
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "searchPathUnavailable");
}

#[test]
fn missing_or_empty_path_cannot_become_an_empty_discovery_result() {
    use jentrics_core::environment::{search_dirs_from, EnvironmentError};
    for path in [None, Some(std::ffi::OsString::new())] {
        assert_eq!(
            search_dirs_from(path),
            Err(EnvironmentError::SearchPathUnavailable)
        );
    }
}

#[test]
fn valid_search_directories_are_preserved() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let path = std::env::join_paths([first.path(), second.path()]).unwrap();
    assert_eq!(
        jentrics_core::environment::search_dirs_from(Some(path)).unwrap(),
        vec![first.path(), second.path()]
    );
}

#[test]
fn invalid_clock_cannot_become_a_fresh_epoch_timestamp() {
    use jentrics_core::environment::{timestamp_ms, EnvironmentError};
    use std::time::{SystemTime, UNIX_EPOCH};
    let invalid: SystemTime = UNIX_EPOCH - Duration::from_secs(1);
    assert_eq!(
        timestamp_ms(invalid),
        Err(EnvironmentError::InvalidSystemTime)
    );
    assert_eq!(
        timestamp_ms(UNIX_EPOCH + Duration::from_millis(1234)),
        Ok(1234)
    );
}

#[tokio::test]
async fn failed_probe_removes_its_temporary_working_directory() {
    let scratch = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_provider_probe"));
    command
        .env("PATH", scratch.path())
        .env("TMP", scratch.path())
        .env("TEMP", scratch.path())
        .env("TMPDIR", scratch.path())
        .current_dir(scratch.path())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .expect("probe did not terminate")
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read_dir(scratch.path()).unwrap().count(),
        0,
        "failed probe must drop its temporary directory"
    );
}
