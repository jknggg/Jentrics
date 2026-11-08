use jentrics_core::storage::{PreferenceKey, Storage};
#[test]
fn preferences_roundtrip_and_history_clear_preserves_settings() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("state.sqlite");
    let mut s = Storage::open(&path).unwrap();
    assert_eq!(s.read_preference(PreferenceKey::Monitor).unwrap(), None);
    s.write_preference(
        PreferenceKey::Monitor,
        &serde_json::json!({"paused":true,"intervalMinutes":5}),
    )
    .unwrap();
    s.clear_history().unwrap();
    drop(s);
    assert_eq!(
        Storage::open(&path)
            .unwrap()
            .read_preference(PreferenceKey::Monitor)
            .unwrap()
            .unwrap()["paused"],
        true
    );
}
