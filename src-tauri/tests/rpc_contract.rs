use jentrics_core::{codex_rpc::read_account, process::ProbeSpec};
use std::{path::PathBuf, time::Duration};

#[tokio::test]
async fn handshake_precedes_read_only_requests_and_identity_is_discarded() {
    let cwd = tempfile::tempdir().unwrap();
    std::fs::write(
        cwd.path().join("limits-response.json"),
        r#"{"result":{"rateLimits":null}}"#,
    )
    .unwrap();
    std::fs::write(cwd.path().join("account-result.json"),
        serde_json::json!({"account":{"type":"chatgpt","email":"fixture@example.invalid"},"requiresOpenaiAuth":true}).to_string()
    ).unwrap();
    let result = read_account(
        ProbeSpec {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_probe-helper")),
            args: vec!["rpc".into()],
            cwd: cwd.path().into(),
            timeout: Duration::from_secs(3),
            max_stream_bytes: 64 * 1024,
        },
        true,
    )
    .await
    .unwrap();
    assert_eq!(result.auth_kind.as_deref(), Some("chatgpt"));
    assert!(result.requires_auth);
    assert_eq!(result.quota.as_ref().unwrap().as_ref().unwrap().len(), 0);
    assert!(!format!("{result:?}").contains("fixture@example.invalid"));
}

#[tokio::test]
async fn missing_account_fixture_cannot_create_a_ready_identity() {
    let cwd = tempfile::tempdir().unwrap();
    std::fs::write(
        cwd.path().join("limits-response.json"),
        r#"{"result":{"rateLimits":null}}"#,
    )
    .unwrap();
    let result = read_account(
        ProbeSpec {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_probe-helper")),
            args: vec!["rpc".into()],
            cwd: cwd.path().into(),
            timeout: Duration::from_secs(3),
            max_stream_bytes: 64 * 1024,
        },
        false,
    )
    .await;
    assert!(
        result.is_err(),
        "missing account fixture must not fabricate a signed-in account"
    );
}

#[tokio::test]
async fn quota_timeout_keeps_readiness_and_reaps_the_process() {
    let cwd = tempfile::tempdir().unwrap();
    std::fs::write(
        cwd.path().join("account-result.json"),
        r#"{"account":{"type":"chatgpt"},"requiresOpenaiAuth":true}"#,
    )
    .unwrap();
    std::fs::write(
        cwd.path().join("limits-response.json"),
        r#"{"fixtureMode":"sleep"}"#,
    )
    .unwrap();
    let result = read_account(
        ProbeSpec {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_probe-helper")),
            args: vec!["rpc".into()],
            cwd: cwd.path().into(),
            timeout: Duration::from_secs(1),
            max_stream_bytes: 64 * 1024,
        },
        true,
    )
    .await
    .unwrap();
    assert_eq!(result.auth_kind.as_deref(), Some("chatgpt"));
    assert_eq!(
        result.quota.unwrap(),
        Err(jentrics_core::process::ProbeError::Timeout)
    );
}
#[tokio::test]
async fn qualified_usage_is_account_scoped_and_failures_preserve_auth_and_quota() {
    use jentrics_core::codex_rpc::read_account_with_usage;
    let cwd = tempfile::tempdir().unwrap();
    std::fs::write(
        cwd.path().join("account-result.json"),
        r#"{"account":{"type":"chatgpt"},"requiresOpenaiAuth":true}"#,
    )
    .unwrap();
    std::fs::write(
        cwd.path().join("limits-response.json"),
        r#"{"result":{"rateLimits":null}}"#,
    )
    .unwrap();
    std::fs::write(
        cwd.path().join("usage-response.json"),
        r#"{"result":{"summary":{"lifetimeTokens":200},"dailyUsageBuckets":[]}}"#,
    )
    .unwrap();
    let result = read_account_with_usage(
        ProbeSpec {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_probe-helper")),
            args: vec!["rpc".into()],
            cwd: cwd.path().into(),
            timeout: Duration::from_secs(3),
            max_stream_bytes: 65536,
        },
        true,
        Some("0.165.0-alpha.1.2"),
    )
    .await
    .unwrap();
    assert_eq!(result.auth_kind.as_deref(), Some("chatgpt"));
    assert!(result.quota.unwrap().is_ok());
    assert_eq!(
        result.usage.unwrap().unwrap().summary.lifetime_tokens,
        Some(200)
    );
    assert!(std::fs::read_to_string(cwd.path().join("rpc-methods.txt"))
        .unwrap()
        .ends_with("account/usage/read\n"));
}
