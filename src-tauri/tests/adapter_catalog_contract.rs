use jentrics_core::{
    adapters::{
        catalog, claude::ClaudeAdapter, codex::CodexAdapter, AgentAdapter, IncrementalUsageError,
    },
    model::*,
};

fn codex(version: &str) -> DashboardState {
    DashboardState {
        schema_version: 1,
        agents: vec![AgentState {
            agent_id: AgentId::Codex,
            version: Some(version.into()),
            availability: Availability::Detected,
            auth_state: AuthState::Ready,
            capabilities: Capabilities::default(),
            observed_ms: 1000,
            diagnostic_code: None,
            quota: None,
            account_usage: None,
        }],
        refreshing: false,
        last_refresh_ms: Some(1000),
        storage_error: None,
        collection_error: None,
    }
}

#[test]
fn catalog_separates_source_capabilities_qualification_and_native_execution() {
    let encoded = serde_json::to_value(catalog(&codex("0.166.0-alpha.5.1"))).unwrap();
    assert_eq!(encoded["schemaVersion"], 1);
    let interfaces = encoded["interfaces"].as_array().unwrap();
    assert_eq!(interfaces.len(), 6);
    let profile = interfaces
        .iter()
        .find(|item| item["source"] == "codexAppServer")
        .unwrap();
    assert_eq!(profile["adapterVersion"], 1);
    assert_eq!(profile["qualification"], "readOnlyProfile");
    assert_eq!(profile["nativeCollectionAllowed"], true);
    for feature in [
        "readiness",
        "quotaWindows",
        "resetTimestamps",
        "accountUsage",
    ] {
        assert_eq!(profile["capabilities"][feature], true);
    }
    for feature in [
        "modelIds",
        "requestDuration",
        "timeToFirstToken",
        "outputRate",
        "incrementalUsageCursor",
        "startSession",
    ] {
        assert_eq!(profile["capabilities"][feature], false);
    }
    let gemini = interfaces
        .iter()
        .find(|item| item["source"] == "geminiOtlpHttpJson")
        .unwrap();
    assert_eq!(gemini["qualification"], "sourceContractOnly");
    assert_eq!(gemini["requiresOptIn"], true);
    assert_eq!(gemini["capabilities"]["tokens"], true);
    assert_eq!(gemini["capabilities"]["meanRequestLatency"], true);
    assert_eq!(gemini["capabilities"]["providerErrorCounts"], true);
    assert_eq!(gemini["nativeCollectionAllowed"], false);
    let opencode = interfaces
        .iter()
        .find(|item| item["adapterId"] == "opencode")
        .unwrap();
    assert_eq!(opencode["qualification"], "startupIsolationUnqualified");
    assert_eq!(opencode["nativeCollectionAllowed"], false);
    assert_eq!(opencode["capabilities"]["startSession"], false);
}

#[test]
fn unknown_versions_never_inherit_a_known_profile_or_account_capability() {
    for version in ["0.166.0", "0.166.0-alpha.5.2", "9.9.9"] {
        let encoded = serde_json::to_value(catalog(&codex(version))).unwrap();
        let profile = encoded["interfaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["source"] == "codexAppServer")
            .unwrap();
        assert_eq!(profile["qualification"], "unsupportedVersion");
        assert_eq!(profile["nativeCollectionAllowed"], false);
        assert_eq!(profile["capabilities"]["accountUsage"], false);
    }
    let encoded = serde_json::to_value(catalog(&codex("0.162.1"))).unwrap();
    let profile = encoded["interfaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["source"] == "codexAppServer")
        .unwrap();
    assert_eq!(profile["capabilities"]["quotaWindows"], true);
    assert_eq!(profile["capabilities"]["accountUsage"], false);
}

#[tokio::test]
async fn native_snapshot_sources_explicitly_decline_incremental_cursors() {
    assert_eq!(
        CodexAdapter.read_usage_since(None).await.unwrap_err(),
        IncrementalUsageError::SourceHasNoCursor
    );
    assert_eq!(
        ClaudeAdapter
            .read_usage_since(Some("untrusted-cursor"))
            .await
            .unwrap_err(),
        IncrementalUsageError::SourceHasNoCursor
    );
}
