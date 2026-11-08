use jentrics_core::{
    discovery::{discover_agent, DiscoveryContext},
    model::{AgentId, AuthState, Availability},
};

#[tokio::test]
#[ignore = "Explicit native qualification: reads the installed Codex account/quota; requires a signed-in supported ChatGPT profile"]
async fn installed_codex_reports_quota_and_account_usage_without_a_model_turn() {
    let directory = tempfile::tempdir().unwrap();
    let context = DiscoveryContext {
        search_dirs: jentrics_core::environment::search_dirs().unwrap(),
        isolated_cwd: directory.path().into(),
    };
    let state = discover_agent(AgentId::Codex, &context, jentrics_core::state::now_ms()).await;
    assert_eq!(state.availability, Availability::Detected);
    assert_eq!(state.auth_state, AuthState::Ready);
    let quota = state.quota.as_ref().expect("Quota capability unavailable");
    assert_eq!(quota.diagnostic_code, None);
    assert!(!quota.windows.is_empty(), "No provider windows returned");
    let usage = state
        .account_usage
        .as_ref()
        .expect("Account usage unavailable");
    assert!(usage.data.is_some(), "Account usage response unavailable");
    assert!(!state.capabilities.session_start);
    // Print capabilities and durations only; account identity and usage totals
    // remain outside the qualification receipt.
    println!(
        "{}",
        serde_json::json!({
            "version":state.version,
            "readiness":"ready",
            "quotaWindowMinutes":quota.windows.iter().map(|window|window.window_minutes).collect::<Vec<_>>(),
            "accountUsageAvailable":true,
            "sessionStartCapability":false
        })
    );
}
