use jentrics_core::claude_bridge::{parse_status_line, BridgeError};

#[test]
fn reported_windows_are_numeric_only_and_missing_is_unknown() {
    let capture = parse_status_line(
        include_bytes!("../../fixtures/claude-statusline.json"),
        1000,
    )
    .expect("documented numeric payload");
    assert_eq!(capture.schema_version, 1);
    assert_eq!(capture.observed_ms, 1000);
    assert_eq!(capture.windows.len(), 2);
    assert_eq!(capture.windows[0].used_percent, Some(23.5));
    assert_eq!(capture.windows[0].resets_at_ms, Some(1_738_425_600_000));
    assert_eq!(capture.windows[1].used_percent, None);
    assert_eq!(capture.windows[1].resets_at_ms, None);
    let serialized = serde_json::to_string(&capture).unwrap();
    for private in ["secret", "cwd", "session", "context", "cost", "model"] {
        assert!(!serialized.contains(private), "discard {private}");
    }
    assert!(parse_status_line(b"{}", 1).unwrap().windows.is_empty());
    assert!(parse_status_line(b"{\"rate_limits\":null}", 1)
        .unwrap()
        .windows
        .is_empty());
}

#[test]
fn malformed_out_of_range_and_oversized_payloads_fail_visibly() {
    for payload in [
        "null",
        "[]",
        "{",
        "{\"rate_limits\":[]}",
        "{\"rate_limits\":{\"five_hour\":{\"used_percentage\":101}}}",
        "{\"rate_limits\":{\"five_hour\":{\"used_percentage\":\"4\"}}}",
        "{\"rate_limits\":{\"five_hour\":{\"resets_at\":-1}}}",
        "{\"rate_limits\":{\"five_hour\":{\"resets_at\":9223372036854775807}}}",
    ] {
        assert!(
            parse_status_line(payload.as_bytes(), 1).is_err(),
            "{payload}"
        );
    }
    assert!(matches!(
        parse_status_line(&vec![b' '; 65537], 1),
        Err(BridgeError::TooLarge)
    ));
    assert!(parse_status_line(b"{}", -1).is_err());
}

#[test]
fn helper_atomically_replaces_latest_and_discards_private_input() {
    use jentrics_core::claude_bridge::ingress::{capture_stdin, read_capture, CAPTURE_FILE};
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    assert_eq!(read_capture(dir.path(), 2000).unwrap(), None);
    capture_stdin(
        include_bytes!("../../fixtures/claude-statusline.json").as_slice(),
        dir.path(),
        1000,
    )
    .unwrap();
    std::fs::write(dir.path().join("unfinished.tmp"), b"{private").unwrap();
    assert_eq!(
        read_capture(dir.path(), 2000).unwrap().unwrap().observed_ms,
        1000
    );
    capture_stdin(
        b"{\"rate_limits\":{\"five_hour\":{\"used_percentage\":42}}}".as_slice(),
        dir.path(),
        2000,
    )
    .unwrap();
    let capture = read_capture(dir.path(), 3000).unwrap().unwrap();
    assert_eq!(capture.windows[0].used_percent, Some(42.0));
    assert!(capture.windows[0].resets_at_ms.is_none());
    assert!(!std::fs::read_to_string(dir.path().join(CAPTURE_FILE))
        .unwrap()
        .contains("secret"));
    assert!(capture_stdin(b"{".as_slice(), dir.path(), 4000).is_err());
    assert!(matches!(
        capture_stdin(vec![b' '; 65537].as_slice(), dir.path(), 4000),
        Err(BridgeError::TooLarge)
    ));
    assert_eq!(
        read_capture(dir.path(), 4000).unwrap().unwrap().observed_ms,
        2000
    );
}

#[test]
fn corrupt_truncated_future_and_old_capture_remain_explicit() {
    use jentrics_core::claude_bridge::ingress::{
        capture_stdin, is_stale, read_capture, CAPTURE_FILE,
    };
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let capture = capture_stdin(b"{}".as_slice(), dir.path(), 1000).unwrap();
    assert!(is_stale(&capture, 900_000, 300_000).unwrap());
    assert!(!is_stale(&capture, 2000, 300_000).unwrap());
    assert!(matches!(
        read_capture(dir.path(), 500),
        Err(BridgeError::ClockChanged)
    ));
    for malformed in [
        "{",
        "{\"schemaVersion\":2,\"observedMs\":1,\"windows\":[]}",
        "{\"schemaVersion\":1,\"observedMs\":1,\"windows\":[],\"prompt\":\"private\"}",
    ] {
        std::fs::write(dir.path().join(CAPTURE_FILE), malformed).unwrap();
        assert!(read_capture(dir.path(), 2000).is_err());
    }
    std::fs::write(dir.path().join(CAPTURE_FILE), vec![b' '; 65537]).unwrap();
    assert!(matches!(
        read_capture(dir.path(), 2000),
        Err(BridgeError::TooLarge)
    ));
}

#[test]
fn setup_is_reviewable_preserves_other_settings_and_restores_exact_bytes() {
    use jentrics_core::claude_bridge::setup::{apply_setup, plan_setup, restore_setup, SetupError};
    use std::path::Path;
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("settings.json");
    let original = b"{ \"env\": {\"SYNTHETIC_PRIVATE\":\"test-value\"}, \"theme\": \"dark\" }\n";
    std::fs::write(&path, original).unwrap();
    let plan = plan_setup(
        Some(original),
        Path::new("C:/Program Files/Jentrics/claude-capture.exe"),
        Path::new("C:/Local/Jentrics/claude-capture"),
    )
    .unwrap();
    assert!(plan.command.contains("powershell"));
    assert!(!plan.command.contains("test-value"));
    assert_eq!(
        apply_setup(&plan, &path, false),
        Err(SetupError::NotConfirmed)
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(!jentrics_core::claude_bridge::setup::backup_path(&path).exists());
    apply_setup(&plan, &path, true).unwrap();
    let installed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(installed["theme"], "dark");
    assert_eq!(installed["env"]["SYNTHETIC_PRIVATE"], "test-value");
    assert_eq!(installed["statusLine"]["type"], "command");
    assert_eq!(installed["statusLine"]["command"], plan.command);
    assert_eq!(restore_setup(&path, false), Err(SetupError::NotConfirmed));
    restore_setup(&path, true).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn fresh_setup_restores_absence_and_existing_backup_is_never_overwritten() {
    use jentrics_core::claude_bridge::setup::{
        apply_setup, backup_path, plan_setup, restore_setup, SetupError,
    };
    use std::path::Path;
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("settings.json");
    let plan = plan_setup(None, Path::new("C:/helper.exe"), Path::new("C:/captures")).unwrap();
    std::fs::write(backup_path(&path), b"preserve existing backup").unwrap();
    assert_eq!(
        apply_setup(&plan, &path, true),
        Err(SetupError::BackupExists)
    );
    assert_eq!(
        std::fs::read(backup_path(&path)).unwrap(),
        b"preserve existing backup"
    );
    assert!(!path.exists());
    std::fs::remove_file(backup_path(&path)).unwrap();
    apply_setup(&plan, &path, true).unwrap();
    restore_setup(&path, true).unwrap();
    assert!(!path.exists());
    assert!(!backup_path(&path).exists());
}

#[test]
fn file_failures_and_invalid_settings_are_explicit() {
    use jentrics_core::claude_bridge::{
        ingress::{read_capture, CAPTURE_FILE},
        setup::{plan_setup, SetupError},
    };
    use std::path::Path;
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    std::fs::create_dir(dir.path().join(CAPTURE_FILE)).unwrap();
    assert!(matches!(
        read_capture(dir.path(), 1000),
        Err(BridgeError::Io)
    ));
    for bytes in [b"{".as_slice(), b"null".as_slice(), b"[]".as_slice()] {
        assert!(matches!(
            plan_setup(
                Some(bytes),
                Path::new("C:/helper.exe"),
                Path::new("C:/captures")
            ),
            Err(SetupError::Malformed)
        ));
    }
    assert!(matches!(
        plan_setup(
            Some(&vec![b' '; 65537]),
            Path::new("C:/helper.exe"),
            Path::new("C:/captures")
        ),
        Err(SetupError::TooLarge)
    ));
}

#[test]
fn existing_status_line_and_unsafe_shell_paths_are_explicitly_declined() {
    use jentrics_core::claude_bridge::setup::{plan_setup, SetupError};
    use std::path::Path;
    assert!(matches!(
        plan_setup(
            Some(br#"{"statusLine":{"type":"command","command":"printf original"}}"#),
            Path::new("C:/helper.exe"),
            Path::new("C:/captures")
        ),
        Err(SetupError::ExistingStatusLine)
    ));
    for path in [
        "C:/bad'path.exe",
        "C:/bad$path.exe",
        "C:/bad`path.exe",
        "C:/bad\"path.exe",
        "relative.exe",
    ] {
        assert!(matches!(
            plan_setup(None, Path::new(path), Path::new("C:/captures")),
            Err(SetupError::UnsafePath)
        ));
    }
}

#[test]
fn setup_rejects_changed_settings_and_restore_preserves_later_user_edits() {
    use jentrics_core::claude_bridge::setup::{apply_setup, plan_setup, restore_setup, SetupError};
    use std::path::Path;
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("settings.json");
    let plan = plan_setup(None, Path::new("C:/helper.exe"), Path::new("C:/captures")).unwrap();
    std::fs::write(&path, b"{}").unwrap();
    assert_eq!(
        apply_setup(&plan, &path, true),
        Err(SetupError::ChangedSinceReview)
    );
    std::fs::remove_file(&path).unwrap();
    apply_setup(&plan, &path, true).unwrap();
    std::fs::write(&path, b"{\"theme\":\"later edit\"}").unwrap();
    assert_eq!(
        restore_setup(&path, true),
        Err(SetupError::ChangedSinceReview)
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"{\"theme\":\"later edit\"}");
}
