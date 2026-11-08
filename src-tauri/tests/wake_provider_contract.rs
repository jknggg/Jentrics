use jentrics_core::{
    model::AgentId,
    wake::{provider::NativeWakeRunner, PreparedWake, WakeReason, WakeRunner},
};
use std::path::Path;
fn install(dir: &Path, agent: AgentId, version: &str) {
    let name = match agent {
        AgentId::Codex => "codex.exe",
        AgentId::Claude => "claude.exe",
    };
    std::fs::copy(env!("CARGO_BIN_EXE_wake-helper"), dir.join(name)).unwrap();
    std::fs::write(dir.join("version.txt"), version).unwrap();
    std::fs::write(dir.join("help.txt"),"--ephemeral --ignore-user-config --ignore-rules --skip-git-repo-check --strict-config --json --bare --restricted --tools --no-session-persistence").unwrap();
    std::fs::write(dir.join("auth.txt"), "Logged in using ChatGPT").unwrap();
    std::fs::write(dir.join("mode.txt"), "success").unwrap();
    std::fs::write(dir.join("catalog.json"),serde_json::json!({"models":[{"slug":"provider-default-fixture","base_instructions":"retain exact provider instructions","tool_mode":"code_mode_only","apply_patch_tool_type":"freeform","multi_agent_version":"v2","experimental_supported_tools":["clock"],"otherMetadata":{"retain":true}}]}).to_string()).unwrap();
}
fn prepared(dir: &Path) -> PreparedWake {
    PreparedWake {
        agent_id: AgentId::Codex,
        executable: std::fs::canonicalize(dir.join("codex.exe")).unwrap(),
        version: "0.165.0-alpha.1.2".into(),
    }
}
fn calls(dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("calls.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn missing_native_or_shell_only_install_cannot_launch_wake() {
    let dir = tempfile::tempdir().unwrap();
    let runner = NativeWakeRunner::new(vec![dir.path().into()]);
    assert!(matches!(
        runner.prepare(AgentId::Codex).await,
        Err(WakeReason::NotInstalled)
    ));
    std::fs::write(dir.path().join("codex.cmd"), "must never execute").unwrap();
    assert!(matches!(
        runner.prepare(AgentId::Codex).await,
        Err(WakeReason::Unsupported)
    ));
}
#[tokio::test]
async fn unqualified_tool_isolation_never_runs_auth_or_sends_a_prompt() {
    for (agent, version) in [
        (AgentId::Codex, "codex-cli 0.162.0-alpha.17.2"),
        (AgentId::Claude, "2.1.287 (Claude Code)"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), agent, version);
        let runner = NativeWakeRunner::new(vec![dir.path().into()]);
        assert!(matches!(
            runner.prepare(agent).await,
            Err(WakeReason::Unsupported)
        ));
        let calls = calls(dir.path());
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["args"], serde_json::json!(["--version"]));
        assert!(!dir.path().join("model-started.txt").exists());
    }
}
#[tokio::test]
async fn current_codex_passes_read_only_checks_but_cannot_launch_with_uncontrollable_retries() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), AgentId::Codex, "codex-cli 0.165.0-alpha.1.2");
    let runner = NativeWakeRunner::new(vec![dir.path().into()]);
    assert!(matches!(
        runner.prepare(AgentId::Codex).await,
        Err(WakeReason::RetryPolicyUnsupported)
    ));
    let calls = calls(dir.path());
    assert_eq!(calls.len(), 4);
    assert!(calls[2]["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "--bundled"));
    assert!(calls[3]["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "status"));
    for call in calls {
        assert_eq!(call["cwdEntries"], 0);
        let cwd = std::path::PathBuf::from(call["cwd"].as_str().unwrap());
        assert!(!cwd.exists());
        assert_ne!(cwd, dir.path());
        assert!(!call["args"].as_array().unwrap().iter().any(|arg| arg
            .as_str()
            .unwrap()
            .starts_with("forced_login_method=")
            || arg == "Reply only with hello."));
    }
    assert!(!dir.path().join("model-started.txt").exists());
}
#[tokio::test]
async fn forged_preparation_cannot_bypass_retry_qualification() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), AgentId::Codex, "codex-cli 0.165.0-alpha.1.2");
    let runner = NativeWakeRunner::new(vec![dir.path().into()]);
    let mut prepared = prepared(dir.path());
    prepared.executable = dir.path().join("arbitrary.exe");
    assert_eq!(
        runner.run(prepared).await,
        Err(WakeReason::RetryPolicyUnsupported)
    );
    assert!(!dir.path().join("model-started.txt").exists());
}
#[tokio::test]
async fn unknown_versions_do_not_pass_to_provider_help_or_auth() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), AgentId::Codex, "codex-cli 9.9.9");
    let runner = NativeWakeRunner::new(vec![dir.path().into()]);
    assert!(matches!(
        runner.prepare(AgentId::Codex).await,
        Err(WakeReason::Unsupported)
    ));
    assert_eq!(calls(dir.path()).len(), 1);
}
#[tokio::test]
async fn auth_is_rechecked_and_api_or_unknown_auth_never_sends_a_prompt() {
    for (status, reason) in [
        ("Not logged in", WakeReason::SignInRequired),
        (
            "Logged in using an API key",
            WakeReason::SubscriptionRequired,
        ),
        ("future-auth", WakeReason::ProviderUnavailable),
    ] {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), AgentId::Codex, "codex-cli 0.165.0-alpha.1.2");
        let runner = NativeWakeRunner::new(vec![dir.path().into()]);
        assert!(matches!(
            runner.prepare(AgentId::Codex).await,
            Err(WakeReason::RetryPolicyUnsupported)
        ));
        std::fs::write(dir.path().join("auth.txt"), status).unwrap();
        assert_eq!(runner.run(prepared(dir.path())).await, Err(reason));
        assert!(!dir.path().join("model-started.txt").exists());
    }
}
#[tokio::test]
async fn changed_version_missing_flags_or_malformed_catalog_never_sends_a_prompt() {
    for changed in ["version", "help", "catalog"] {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), AgentId::Codex, "codex-cli 0.165.0-alpha.1.2");
        let runner = NativeWakeRunner::new(vec![dir.path().into()]);
        let (file, text) = match changed {
            "version" => ("version.txt", "codex-cli 0.165.1"),
            "help" => ("help.txt", "--ephemeral --json"),
            _ => (
                "catalog.json",
                "{\"models\":[{\"slug\":\"unknown-tool-schema\"}]}",
            ),
        };
        std::fs::write(dir.path().join(file), text).unwrap();
        assert_eq!(
            runner.run(prepared(dir.path())).await,
            Err(WakeReason::Unsupported)
        );
        assert!(!dir.path().join("model-started.txt").exists());
    }
}
#[test]
fn inherited_api_credentials_and_route_overrides_are_rejected_before_model_launch() {
    for name in [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "OPENAI_BASE_URL",
        "CODEX_ACCESS_TOKEN",
    ] {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), AgentId::Codex, "codex-cli 0.165.0-alpha.1.2");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "environment_override_child", "--nocapture"])
            .env("WAKE_ENV_TEST_DIR", dir.path())
            .env(name, "fixture-value-not-a-real-key")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child failed: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(!dir.path().join("model-started.txt").exists());
    }
}
#[tokio::test]
async fn environment_override_child() {
    let Some(dir) = std::env::var_os("WAKE_ENV_TEST_DIR") else {
        return;
    };
    let runner = NativeWakeRunner::new(vec![dir.into()]);
    assert!(matches!(
        runner.prepare(AgentId::Codex).await,
        Err(WakeReason::SubscriptionRequired)
    ));
}

#[tokio::test]
async fn native_preflight_is_bounded_and_never_sends_a_prompt_on_failure() {
    for (mode, reason) in [
        ("preflight-stdout-limit", WakeReason::OutputLimit),
        ("preflight-stderr-limit", WakeReason::OutputLimit),
        ("preflight-timeout", WakeReason::TimedOut),
    ] {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), AgentId::Codex, "codex-cli 0.165.0-alpha.1.2");
        std::fs::write(dir.path().join("mode.txt"), mode).unwrap();
        let runner = NativeWakeRunner::new(vec![dir.path().into()]);
        let result = runner.prepare(AgentId::Codex).await;
        assert!(matches!(result,Err(actual) if actual==reason));
        assert!(!dir.path().join("model-started.txt").exists());
        for call in calls(dir.path()) {
            assert!(!std::path::Path::new(call["cwd"].as_str().unwrap()).exists());
        }
    }
}
