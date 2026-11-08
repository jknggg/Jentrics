use jentrics_core::claude_bridge::control::{BridgeControl, BridgeIssue};
#[test]
fn reviewed_setup_is_one_use_and_restorable_without_leaking_settings() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let settings = dir.path().join("settings.json");
    std::fs::write(
        &settings,
        b"{\"theme\":\"dark\",\"privateToken\":\"SECRET\"}",
    )
    .unwrap();
    let control = BridgeControl::new(
        settings.clone(),
        dir.path().join("capture"),
        std::env::current_exe().unwrap(),
    );
    assert!(control.status(1000).can_setup);
    let review = control.prepare(1000).unwrap();
    assert!(!serde_json::to_string(&review).unwrap().contains("SECRET"));
    assert_eq!(
        control
            .apply(&review.confirmation_id, false, 1001)
            .unwrap_err(),
        BridgeIssue::ConfirmationRequired
    );
    assert!(!settings
        .with_extension("json.jentrics-backup.json")
        .exists());
    let review = control.prepare(1002).unwrap();
    let status = control.apply(&review.confirmation_id, true, 1003).unwrap();
    assert_eq!(status.configured, Some(true));
    assert_eq!(
        control
            .apply(&review.confirmation_id, true, 1004)
            .unwrap_err(),
        BridgeIssue::ChangedSinceReview
    );
    assert_eq!(
        control.restore(false, 1005).unwrap_err(),
        BridgeIssue::ConfirmationRequired
    );
    control.restore(true, 1006).unwrap();
    assert_eq!(
        std::fs::read(&settings).unwrap(),
        b"{\"theme\":\"dark\",\"privateToken\":\"SECRET\"}"
    );
}
#[test]
fn expiry_changed_config_and_existing_status_are_explicit() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let settings = dir.path().join("settings.json");
    let control = BridgeControl::new(
        settings.clone(),
        dir.path().join("capture"),
        std::env::current_exe().unwrap(),
    );
    let review = control.prepare(1000).unwrap();
    assert_eq!(
        control
            .apply(&review.confirmation_id, true, 121001)
            .unwrap_err(),
        BridgeIssue::ExpiredReview
    );
    let review = control.prepare(200000).unwrap();
    std::fs::write(&settings, b"{\"statusLine\":null}").unwrap();
    assert_eq!(
        control
            .apply(&review.confirmation_id, true, 200001)
            .unwrap_err(),
        BridgeIssue::ChangedSinceReview
    );
    assert_eq!(
        control.status(200002).issue,
        Some(BridgeIssue::ExistingStatusLine)
    );
}
#[test]
fn matching_command_with_wrong_status_line_type_is_not_configured() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let settings = dir.path().join("settings.json");
    let control = BridgeControl::new(
        settings.clone(),
        dir.path().join("capture"),
        std::env::current_exe().unwrap(),
    );
    let review = control.prepare(1000).unwrap();
    std::fs::write(
        settings,
        serde_json::json!({"statusLine":{"type":"invalid","command":review.command}}).to_string(),
    )
    .unwrap();
    let status = control.status(1001);
    assert_eq!(status.configured, Some(false));
    assert_eq!(status.issue, Some(BridgeIssue::ExistingStatusLine));
}
