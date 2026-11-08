use jentrics_core::{
    claude_bridge::{self, ingress, parse_status_line},
    model::*,
    storage::Storage,
};
#[test]
fn claude_capture_keeps_original_time_and_roundtrips_history() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let capture_dir = dir.path().join("capture");
    ingress::write_capture(
        &capture_dir,
        &parse_status_line(
            br#"{"rate_limits":{"five_hour":{"used_percentage":42,"resets_at":200}}}"#,
            1000,
        )
        .unwrap(),
    )
    .unwrap();
    let mut agent:AgentState=serde_json::from_value(serde_json::json!({"agentId":"claude","version":"2.1.0","availability":"detected","authState":"ready","capabilities":{"discovery":true,"authReadiness":true,"quotas":false,"usage":false,"sessionStart":false},"observedMs":10000,"diagnosticCode":null,"quota":null})).unwrap();
    claude_bridge::runtime::attach_capture(&mut agent, &capture_dir, 10000).unwrap();
    assert_eq!(agent.quota.as_ref().unwrap().observed_ms, 1000);
    let mut storage = Storage::open(&dir.path().join("state.sqlite")).unwrap();
    storage.save_agents(&[agent.clone()]).unwrap();
    assert_eq!(storage.load_agents().unwrap(), vec![agent]);
    assert_eq!(
        storage.history(24, 10000).unwrap().samples[0].observed_ms,
        1000
    );
}
