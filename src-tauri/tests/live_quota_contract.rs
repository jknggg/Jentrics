use jentrics_core::{
    discovery::{discover_agent, DiscoveryContext},
    model::AgentId,
};
use serde_json::{json, Value};

fn setup(limits: Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_probe-helper"),
        dir.path().join("codex.exe"),
    )
    .unwrap();
    for (name, content) in [
        ("version.txt", "codex-cli 0.162.0-alpha.17.2".to_owned()),
        ("help.txt", "--listen --strict-config".to_owned()),
        ("account-result.json", json!({"account":{"type":"chatgpt","email":"discard@example.invalid"},"requiresOpenaiAuth":true}).to_string()),
        ("limits-response.json", limits.to_string()),
    ] { std::fs::write(dir.path().join(name), content).unwrap(); }
    dir
}
async fn collect(dir: &tempfile::TempDir) -> Value {
    let state = discover_agent(
        AgentId::Codex,
        &DiscoveryContext {
            search_dirs: vec![dir.path().into()],
            isolated_cwd: dir.path().into(),
        },
        1000,
    )
    .await;
    serde_json::to_value(state).unwrap()
}
#[tokio::test]
async fn discovery_delivers_sanitized_multi_bucket_quota_in_same_readiness_check() {
    let dir = setup(json!({"result":{
        "rateLimits":{"primary":{"usedPercent":99}},
        "rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":25,"windowDurationMins":300,"resetsAt":1800000000},"secondary":{"usedPercent":null}}}
    }}));
    let state = collect(&dir).await;
    assert_eq!(state["authState"], "ready");
    assert_eq!(state["capabilities"]["quotas"], true);
    assert_eq!(
        state["quota"]["windows"][0]["usedPercent"].as_f64(),
        Some(25.0)
    );
    assert_eq!(
        state["quota"]["windows"][0]["resetsAtMs"],
        1800000000000_i64
    );
    assert!(state["quota"]["windows"][1]["usedPercent"].is_null());
    assert_eq!(state["quota"]["source"], "codexAppServer");
    assert!(!state.to_string().contains("discard@example.invalid"));
    let calls = std::fs::read_to_string(dir.path().join("rpc-methods.txt")).unwrap();
    assert_eq!(
        calls.lines().collect::<Vec<_>>(),
        vec![
            "initialize",
            "initialized",
            "account/read",
            "account/rateLimits/read"
        ]
    );
}
#[tokio::test]
async fn quota_errors_do_not_erase_successful_authentication() {
    for response in [
        json!({"result":{"rateLimits":{"primary":{"windowDurationMins":9007199254740992_i64}}}}),
        json!({"error":{"code":-1,"message":"private provider text"}}),
        json!({"result":{"rateLimits":{"primary":{"usedPercent":101}}}}),
    ] {
        let dir = setup(response);
        let state = collect(&dir).await;
        assert_eq!(state["authState"], "ready");
        assert_eq!(state["availability"], "detected");
        assert_eq!(state["quota"]["diagnosticCode"], "quotaUnavailable");
        assert_eq!(state["quota"]["windows"], json!([]));
        assert!(!state.to_string().contains("private provider text"));
    }
}
#[tokio::test]
async fn missing_quota_is_distinct_from_zero_usage() {
    for (payload, expected) in [
        (json!({"rateLimits":null}), json!([])),
        (
            json!({"rateLimits":{"primary":{"usedPercent":0}}}),
            json!([0.0]),
        ),
    ] {
        let dir = setup(json!({"result":payload}));
        let state = collect(&dir).await;
        let values: Vec<Value> = state["quota"]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["usedPercent"].clone())
            .collect();
        assert_eq!(json!(values), expected);
        assert!(state["quota"]["diagnosticCode"].is_null());
    }
}
#[tokio::test]
async fn feasibility_probe_still_fails_when_the_quota_read_fails() {
    let dir = setup(json!({"error":{"code":-1,"message":"unavailable"}}));
    let result = jentrics_core::discovery::probe_codex_limits(&DiscoveryContext {
        search_dirs: vec![dir.path().into()],
        isolated_cwd: dir.path().into(),
    })
    .await;
    assert!(
        result.is_err(),
        "developer feasibility probe must not report successful quota collection"
    );
}

#[tokio::test]
async fn qualified_profiles_keep_account_reads_fixed_and_discard_new_metadata() {
    for version in ["0.166.0-alpha.5.1", "0.167.0-alpha.3.2"] {
        let dir = setup(json!({"result":{"rateLimitsByLimitId":{"codex":{
            "primary":null,"secondary":{"usedPercent":12,"windowDurationMins":10080}
        }}}}));
        std::fs::write(
            dir.path().join("version.txt"),
            format!("codex-cli {version}"),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("account-result.json"),
            json!({
                "account":{"type":"chatgpt","workspaceName":"PRIVATE_WORKSPACE"},
                "requiresOpenaiAuth":true
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("usage-response.json"),
            json!({"result":{
                "summary":{"lifetimeTokens":0},"dailyUsageBuckets":[],
                "threadUsage":{"threadId":"PRIVATE_THREAD","nativeUsageUsdMicros":123}
            }})
            .to_string(),
        )
        .unwrap();
        let state = collect(&dir).await;
        assert_eq!(state["availability"], "detected");
        assert_eq!(state["authState"], "ready");
        assert_eq!(state["quota"]["windows"][0]["windowMinutes"], 10080);
        assert_eq!(
            state["accountUsage"]["data"]["summary"]["lifetimeTokens"],
            0
        );
        assert_eq!(state["capabilities"]["sessionStart"], false);
        assert!(!state.to_string().contains("PRIVATE_"));
        let calls = std::fs::read_to_string(dir.path().join("rpc-methods.txt")).unwrap();
        assert_eq!(
            calls.lines().collect::<Vec<_>>(),
            vec![
                "initialize",
                "initialized",
                "account/read",
                "account/rateLimits/read",
                "account/usage/read"
            ]
        );
    }
}
