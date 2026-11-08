use jentrics_core::{
    adapters::codex::QuotaWindow,
    codex_usage::{
        AccountUsageSummary, CodexAccountUsage, DailyUsageBucket, UsageScope, UsageSnapshot,
        UsageSource,
    },
    model::*,
    storage::Storage,
};

fn agent(id: AgentId, at: i64) -> AgentState {
    let source = if id == AgentId::Claude {
        QuotaSource::ClaudeStatusLine
    } else {
        QuotaSource::CodexAppServer
    };
    AgentState {
        agent_id: id,
        version: Some(
            if id == AgentId::Claude {
                "2.1.0"
            } else {
                "0.165.0-alpha.1.2"
            }
            .into(),
        ),
        availability: Availability::Detected,
        auth_state: AuthState::Ready,
        capabilities: Capabilities {
            discovery: true,
            auth_readiness: true,
            quotas: true,
            usage: false,
            session_start: false,
        },
        observed_ms: at,
        diagnostic_code: None,
        account_usage: None,
        quota: Some(QuotaSnapshot {
            source,
            observed_ms: at,
            diagnostic_code: None,
            windows: vec![QuotaWindow {
                bucket_id: if id == AgentId::Claude {
                    "claude:five_hour"
                } else {
                    "codex:primary"
                }
                .into(),
                window_minutes: Some(300),
                used_percent: Some(20.0),
                resets_at_ms: Some(500_000),
                observed_ms: at,
            }],
        }),
    }
}

#[test]
fn claude_capture_recovery_does_not_overwrite_failed_check_at_readiness_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&dir.path().join("review.sqlite")).unwrap();
    let mut latest = agent(AgentId::Claude, 500);
    // A saved readiness failure is independent of subsequent captured quota.
    latest.availability = Availability::ProbeFailed;
    latest.auth_state = AuthState::Unknown;
    latest.diagnostic_code = Some(DiagnosticCode::AuthUnavailable);
    {
        let quota = latest.quota.as_mut().unwrap();
        quota.observed_ms = 1000;
        quota.windows[0].observed_ms = 1000;
    }
    storage.save_agents(&[latest.clone()]).unwrap();
    {
        let quota = latest.quota.as_mut().unwrap();
        quota.observed_ms = 2000;
        quota.windows.clear();
        quota.diagnostic_code = Some(DiagnosticCode::QuotaUnavailable);
    }
    storage.save_agents(&[latest.clone()]).unwrap();
    {
        let quota = latest.quota.as_mut().unwrap();
        quota.observed_ms = 3000;
        quota.diagnostic_code = None;
        quota.windows = vec![QuotaWindow {
            bucket_id: "claude:five_hour".into(),
            window_minutes: Some(300),
            used_percent: Some(21.0),
            resets_at_ms: Some(500_000),
            observed_ms: 3000,
        }];
    }
    storage.save_agents(&[latest.clone()]).unwrap();
    let history = storage.history(24, 4000).unwrap();
    assert!(history.checks.iter().any(|check| check.observed_ms == 2000
        && check.status == "error"
        && check.diagnostic_code == Some(DiagnosticCode::QuotaUnavailable)));
    assert!(history.checks.iter().any(|check| check.observed_ms == 3000
        && check.status == "reported"
        && check.diagnostic_code.is_none()));
    assert!(history.checks.iter().all(|check| check.observed_ms != 500));
    assert_eq!(storage.load_agents().unwrap(), vec![latest]);
    assert_eq!(history.samples[0].observed_ms, 3000);
}

#[test]
fn valid_account_daily_buckets_above_old_state_limit_persist_with_entire_provider_batch() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&dir.path().join("review.sqlite")).unwrap();
    let mut codex = agent(AgentId::Codex, 1000);
    let buckets = (0..400)
        .map(|index| DailyUsageBucket {
            start_date: format!(
                "{:04}-{:02}-{:02}",
                2025 + index / 336,
                index % 336 / 28 + 1,
                index % 28 + 1
            ),
            tokens: 1_000_000,
        })
        .collect();
    codex.capabilities.usage = true;
    codex.account_usage = Some(UsageSnapshot {
        error: None,
        data: Some(CodexAccountUsage {
            source: UsageSource::CodexAppServer,
            scope: UsageScope::Account,
            observed_ms: 1000,
            summary: AccountUsageSummary {
                lifetime_tokens: Some(400_000_000),
                peak_daily_tokens: Some(1_000_000),
                longest_running_turn_sec: None,
                current_streak_days: None,
                longest_streak_days: None,
            },
            daily_usage_buckets: Some(buckets),
        }),
    });
    let encoded = serde_json::to_vec(&codex).unwrap();
    assert!(encoded.len() > 16 * 1024);
    assert!(encoded.len() < 64 * 1024);
    let claude = agent(AgentId::Claude, 1000);
    storage
        .save_agents(&[codex.clone(), claude.clone()])
        .unwrap();
    assert_eq!(storage.load_agents().unwrap(), vec![claude, codex]);
    assert_eq!(storage.history(24, 2000).unwrap().samples.len(), 2);
}

#[cfg(windows)]
#[test]
fn reviewed_statusline_command_waits_for_gui_helper_and_propagates_its_exit_status() {
    use std::{
        io::Write,
        os::windows::process::CommandExt,
        path::PathBuf,
        process::{Command, Stdio},
    };
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let helper = dir.path().join("synthetic gui helper.exe");
    let source = dir.path().join("synthetic_gui_helper.rs");
    // This fixture models only the Windows GUI process/stdio boundary. Capture
    // JSON parsing and privacy are exercised separately against capture_cli.
    std::fs::write(
        &source,
        r##"
#![windows_subsystem="windows"]
use std::io::Read;
fn main() {
 let mut input=String::new(); std::io::stdin().read_to_string(&mut input).unwrap();
 if input.trim()==r#"{"valid":true}"# { println!("synthetic capture"); }
 else { eprintln!("synthetic capture failure");std::process::exit(1); }
}
"##,
    )
    .unwrap();
    let compiled = Command::new("rustc")
        .args(["--edition", "2021", "--crate-name", "synthetic_gui_helper"])
        .arg(&source)
        .arg("-o")
        .arg(&helper)
        .creation_flags(0x0800_0000)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let capture = dir.path().join("synthetic capture directory");
    let plan = jentrics_core::claude_bridge::setup::plan_setup(None, &helper, &capture).unwrap();
    let powershell =
        PathBuf::from(std::env::var_os("SystemRoot").expect("Windows SystemRoot is required"))
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut shells = vec![(
        powershell,
        vec!["-NoProfile", "-NonInteractive", "-Command"],
    )];
    // MSYS named-object creation is blocked by some Windows test sandboxes.
    // Qualify Git Bash explicitly outside that sandbox; a requested run must pass.
    if let Some(path) = std::env::var_os("JENTRICS_TEST_GIT_BASH") {
        let git_bash = PathBuf::from(path);
        assert!(git_bash.is_file(), "Git Bash qualification path must exist");
        shells.push((git_bash, vec!["-c"]));
    } else {
        eprintln!("Git Bash qualification not requested; this run covers PowerShell only.");
    }
    for (shell, args) in shells {
        for (input, code) in [(b"{\"valid\":true}".as_slice(), 0), (b"{".as_slice(), 1)] {
            let mut child = Command::new(&shell)
                .args(&args)
                .arg(&plan.command)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .creation_flags(0x0800_0000)
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(input).unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(code),
                "shell={shell:?}, stderr={}",
                String::from_utf8_lossy(&output.stderr)
            );
            if code == 0 {
                assert!(String::from_utf8_lossy(&output.stdout).contains("synthetic capture"));
            }
        }
    }
}
