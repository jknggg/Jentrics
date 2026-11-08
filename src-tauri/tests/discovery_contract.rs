use jentrics_core::{
    adapters::claude::parse_auth,
    discovery::{parse_version, resolve_native, version_supported, DiscoveryError},
    model::{AgentId, AuthState},
    process::ProbeOutput,
};

#[tokio::test]
async fn only_explicit_null_account_can_be_reported_as_not_authenticated() {
    for (account, expected) in [
        (serde_json::json!({"type":"futureAuth"}), AuthState::Unknown),
        (serde_json::json!({}), AuthState::Unknown),
        (serde_json::json!(false), AuthState::Unknown),
        (serde_json::Value::Null, AuthState::NotAuthenticated),
        (serde_json::json!({"type":"chatgpt"}), AuthState::Ready),
    ] {
        let dir = tempfile::tempdir().unwrap();
        install_cli_fixture(dir.path());
        std::fs::write(
            dir.path().join("account-result.json"),
            serde_json::json!({"account":account,"requiresOpenaiAuth":true}).to_string(),
        )
        .unwrap();
        let result = jentrics_core::discovery::discover_agent(
            AgentId::Codex,
            &jentrics_core::discovery::DiscoveryContext {
                search_dirs: vec![dir.path().into()],
                isolated_cwd: dir.path().into(),
            },
            1000,
        )
        .await;
        assert_eq!(result.auth_state, expected, "account: {account}");
    }
}

#[tokio::test]
async fn feasibility_probe_refuses_unknown_versions_and_missing_isolation_flags() {
    for (version, help) in [
        ("codex-cli 9.9.9", "--listen --strict-config"),
        ("codex-cli 0.162.0-alpha.17.2", "--listen"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        install_cli_fixture(dir.path());
        std::fs::write(dir.path().join("version.txt"), version).unwrap();
        std::fs::write(dir.path().join("help.txt"), help).unwrap();
        let result = jentrics_core::discovery::probe_codex_limits(
            &jentrics_core::discovery::DiscoveryContext {
                search_dirs: vec![dir.path().into()],
                isolated_cwd: dir.path().into(),
            },
        )
        .await;
        assert!(result.is_err());
        assert!(!dir.path().join("app-server-started.txt").exists());
    }
}

#[tokio::test]
async fn feasibility_probe_reads_limits_after_compatibility_gates() {
    let dir = tempfile::tempdir().unwrap();
    install_cli_fixture(dir.path());
    std::fs::write(
        dir.path().join("account-result.json"),
        serde_json::json!({"account":null,"requiresOpenaiAuth":true}).to_string(),
    )
    .unwrap();
    let result =
        jentrics_core::discovery::probe_codex_limits(&jentrics_core::discovery::DiscoveryContext {
            search_dirs: vec![dir.path().into()],
            isolated_cwd: dir.path().into(),
        })
        .await
        .unwrap();
    assert!(result.account_is_null);
    assert_eq!(result.quota.unwrap().unwrap().len(), 0);
    assert!(dir.path().join("app-server-started.txt").exists());
}

#[tokio::test]
async fn unknown_versions_keep_metadata_and_do_not_attempt_auth() {
    let dir = tempfile::tempdir().unwrap();
    install_cli_fixture(dir.path());
    std::fs::write(dir.path().join("version.txt"), "codex-cli 9.9.9").unwrap();
    let result = jentrics_core::discovery::discover_agent(
        AgentId::Codex,
        &jentrics_core::discovery::DiscoveryContext {
            search_dirs: vec![dir.path().into()],
            isolated_cwd: dir.path().into(),
        },
        1000,
    )
    .await;
    assert_eq!(result.version.as_deref(), Some("9.9.9"));
    assert_eq!(
        result.availability,
        jentrics_core::model::Availability::Unsupported
    );
    assert_eq!(result.auth_state, AuthState::Unknown);
}

#[tokio::test]
async fn malformed_readiness_never_becomes_logged_out() {
    let dir = tempfile::tempdir().unwrap();
    install_cli_fixture(dir.path());
    let result = jentrics_core::discovery::discover_agent(
        AgentId::Codex,
        &jentrics_core::discovery::DiscoveryContext {
            search_dirs: vec![dir.path().into()],
            isolated_cwd: dir.path().into(),
        },
        1000,
    )
    .await;
    assert_eq!(
        result.availability,
        jentrics_core::model::Availability::ProbeFailed
    );
    assert_eq!(result.auth_state, AuthState::Unknown);
    assert_eq!(result.version.as_deref(), Some("0.162.0-alpha.17.2"));
}

#[test]
fn empty_path_is_not_a_login_failure() {
    assert_eq!(
        resolve_native(AgentId::Codex, &[]),
        Err(DiscoveryError::NotFound)
    );
}
#[test]
fn shell_shim_only_install_is_unavailable_and_not_executed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("codex.cmd"), "echo should not execute").unwrap();
    assert_eq!(
        resolve_native(AgentId::Codex, &[dir.path().into()]),
        Err(DiscoveryError::Unavailable)
    );
}
#[test]
fn detected_executable_is_canonical_and_native() {
    let dir = tempfile::tempdir().unwrap();
    install_cli_fixture(dir.path());
    assert_eq!(
        resolve_native(AgentId::Codex, &[dir.path().into()]).unwrap(),
        std::fs::canonicalize(dir.path().join("codex.exe")).unwrap()
    );
}
#[test]
fn unsupported_version_is_retained_but_not_authenticated() {
    assert_eq!(
        parse_version(AgentId::Codex, b"codex-cli 9.9.9\n"),
        Some("9.9.9".into())
    );
    assert!(!version_supported(AgentId::Codex, "9.9.9"));
    assert!(version_supported(AgentId::Codex, "0.162.0-alpha.17.2"));
    assert!(version_supported(AgentId::Claude, "2.1.287"));
    assert_eq!(
        parse_version(AgentId::Claude, b"error at private/path"),
        None
    );
}
#[test]
fn auth_identity_is_discarded_and_exit_status_is_validated() {
    let output = ProbeOutput {
        exit_code: Some(0),
        stdout: include_bytes!("../../fixtures/claude-auth.json").to_vec(),
        stderr: vec![],
    };
    assert_eq!(parse_auth(&output).unwrap(), AuthState::Ready);
    for (exit, text) in [
        (0, "{}"),
        (1, "{\"loggedIn\":true}"),
        (2, "{\"loggedIn\":false}"),
        (0, "not json"),
    ] {
        assert!(parse_auth(&ProbeOutput {
            exit_code: Some(exit),
            stdout: text.as_bytes().to_vec(),
            stderr: vec![]
        })
        .is_err());
    }
    assert_eq!(
        parse_auth(&ProbeOutput {
            exit_code: Some(1),
            stdout: b"{\"loggedIn\":false}".to_vec(),
            stderr: vec![]
        })
        .unwrap(),
        AuthState::NotAuthenticated
    );
}

#[tokio::test]
async fn missing_version_fixture_cannot_report_a_supported_cli() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_probe-helper"),
        dir.path().join("codex.exe"),
    )
    .unwrap();
    let output = jentrics_core::process::run_probe(jentrics_core::process::ProbeSpec {
        executable: dir.path().join("codex.exe"),
        args: vec!["--version".into()],
        cwd: dir.path().into(),
        timeout: std::time::Duration::from_secs(3),
        max_stream_bytes: 64 * 1024,
    })
    .await
    .unwrap();
    assert_ne!(
        output.exit_code,
        Some(0),
        "missing fixture must fail, not fabricate a known version"
    );
}

fn install_cli_fixture(directory: &std::path::Path) {
    std::fs::write(
        directory.join("limits-response.json"),
        r#"{"result":{"rateLimits":null}}"#,
    )
    .unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_probe-helper"),
        directory.join("codex.exe"),
    )
    .unwrap();
    std::fs::write(
        directory.join("version.txt"),
        "codex-cli 0.162.0-alpha.17.2",
    )
    .unwrap();
    std::fs::write(directory.join("help.txt"), "--listen --strict-config").unwrap();
}
#[test]
fn exact_installed_profile_is_qualified_without_allowing_unverified_minor_versions() {
    assert!(version_supported(AgentId::Codex, "0.165.0-alpha.1.2"));
    assert!(version_supported(AgentId::Codex, "0.166.0-alpha.5.1"));
    assert!(version_supported(AgentId::Codex, "0.167.0-alpha.3.2"));
    assert!(!version_supported(AgentId::Codex, "0.165.1"));
    assert!(!version_supported(AgentId::Codex, "0.166.0"));
    assert!(!version_supported(AgentId::Codex, "0.166.0-alpha.5.2"));
    assert!(!version_supported(AgentId::Codex, "0.167.0"));
    assert!(!version_supported(AgentId::Codex, "0.167.0-alpha.3.3"));
}
