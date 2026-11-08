use crate::model::AgentId;
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub struct DiscoveryContext {
    pub search_dirs: Vec<PathBuf>,
    pub isolated_cwd: PathBuf,
}
#[derive(Debug, PartialEq, Eq)]
pub enum DiscoveryError {
    NotFound,
    Unavailable,
}

pub async fn discover_agent(
    id: AgentId,
    context: &DiscoveryContext,
    observed_ms: i64,
) -> crate::model::AgentState {
    use crate::{
        model::{AgentState, AuthState, Availability, Capabilities, DiagnosticCode},
        process::{run_probe, ProbeError},
    };
    use std::time::Duration;
    let mut state = AgentState {
        agent_id: id,
        version: None,
        availability: Availability::NotFound,
        auth_state: AuthState::Unknown,
        capabilities: Capabilities {
            discovery: true,
            ..Default::default()
        },
        observed_ms,
        diagnostic_code: None,
        quota: None,
        account_usage: None,
    };
    let executable = match resolve_native(id, &context.search_dirs) {
        Ok(path) => path,
        Err(DiscoveryError::NotFound) => return state,
        Err(DiscoveryError::Unavailable) => {
            state.availability = Availability::ProbeFailed;
            state.diagnostic_code = Some(DiagnosticCode::ExecutableUnavailable);
            return state;
        }
    };
    let spec = |args: &[&str]| probe_spec(&executable, context, args);
    let version = checked_version(id, &executable, context).await;
    let Some(version) = version else {
        state.availability = Availability::ProbeFailed;
        state.diagnostic_code = Some(DiagnosticCode::VersionUnavailable);
        return state;
    };
    state.version = Some(version.clone());
    if !version_supported(id, &version) {
        state.availability = Availability::Unsupported;
        state.diagnostic_code = Some(DiagnosticCode::UnsupportedVersion);
        return state;
    }
    state.capabilities.auth_readiness = true;
    let supported = checked_help(id, &executable, context).await;
    if !supported {
        state.availability = Availability::ProbeFailed;
        state.diagnostic_code = Some(DiagnosticCode::AuthUnavailable);
        return state;
    }
    let readiness = match id {
        AgentId::Claude => match run_probe(spec(&["auth", "status"])).await {
            Ok(output) => {
                crate::adapters::claude::parse_auth(&output).map_err(|_| ProbeError::ProtocolFailed)
            }
            Err(error) => Err(error),
        },
        AgentId::Codex => {
            let mut input = spec(&[]);
            input.args = crate::codex_rpc::isolated_args();
            input.timeout = Duration::from_secs(15);
            crate::codex_rpc::read_account_with_usage(input, true, Some(&version))
                .await
                .map(|observation| {
                    if let Some(result) = observation.usage {
                        state.capabilities.usage = true;
                        state.account_usage = Some(match result {
                            Ok(data) => crate::codex_usage::UsageSnapshot {
                                data: Some(data),
                                error: None,
                            },
                            Err(error) => crate::codex_usage::UsageSnapshot {
                                data: None,
                                error: Some(error),
                            },
                        });
                    }
                    if let Some(result) = observation.quota {
                        let (windows, diagnostic_code) = match result {
                            Ok(mut windows) => {
                                for window in &mut windows {
                                    window.observed_ms = observed_ms;
                                }
                                (windows, None)
                            }
                            Err(error) => (
                                vec![],
                                Some(match error {
                                    ProbeError::Timeout => DiagnosticCode::ProbeTimeout,
                                    ProbeError::OutputLimit => DiagnosticCode::ProbeOutputLimit,
                                    _ => DiagnosticCode::QuotaUnavailable,
                                }),
                            ),
                        };
                        state.capabilities.quotas = true;
                        state.quota = Some(crate::model::QuotaSnapshot {
                            source: crate::model::QuotaSource::CodexAppServer,
                            observed_ms,
                            windows,
                            diagnostic_code,
                        });
                    }
                    if observation.auth_kind.is_some() {
                        AuthState::Ready
                    } else if observation.account_is_null && observation.requires_auth {
                        AuthState::NotAuthenticated
                    } else {
                        AuthState::Unknown
                    }
                })
        }
    };
    match readiness {
        Ok(auth) => {
            state.availability = Availability::Detected;
            state.auth_state = auth;
        }
        Err(error) => {
            state.availability = Availability::ProbeFailed;
            state.diagnostic_code = Some(match error {
                ProbeError::Timeout => DiagnosticCode::ProbeTimeout,
                ProbeError::OutputLimit => DiagnosticCode::ProbeOutputLimit,
                _ => DiagnosticCode::AuthUnavailable,
            });
        }
    }
    state
}

fn probe_spec(
    executable: &Path,
    context: &DiscoveryContext,
    args: &[&str],
) -> crate::process::ProbeSpec {
    crate::process::ProbeSpec {
        executable: executable.into(),
        args: args.iter().map(Into::into).collect(),
        cwd: context.isolated_cwd.clone(),
        timeout: std::time::Duration::from_secs(5),
        max_stream_bytes: 64 * 1024,
    }
}

async fn checked_version(
    id: AgentId,
    executable: &Path,
    context: &DiscoveryContext,
) -> Option<String> {
    match crate::process::run_probe(probe_spec(executable, context, &["--version"])).await {
        Ok(output) if output.exit_code == Some(0) => parse_version(id, &output.stdout),
        _ => None,
    }
}

async fn checked_help(id: AgentId, executable: &Path, context: &DiscoveryContext) -> bool {
    let args = match id {
        AgentId::Claude => ["auth", "--help"],
        AgentId::Codex => ["app-server", "--help"],
    };
    crate::process::run_probe(probe_spec(executable, context, &args))
        .await
        .is_ok_and(|o| {
            o.exit_code == Some(0)
                && std::str::from_utf8(&o.stdout).is_ok_and(|text| match id {
                    AgentId::Claude => text.split_whitespace().any(|word| word == "status"),
                    AgentId::Codex => text.contains("--listen") && text.contains("--strict-config"),
                })
        })
}

/// Developer quota feasibility uses the same native/version/isolation gates as readiness.
pub async fn probe_codex_limits(
    context: &DiscoveryContext,
) -> Result<crate::codex_rpc::CodexObservation, crate::model::DiagnosticCode> {
    use crate::model::DiagnosticCode;
    let executable = resolve_native(AgentId::Codex, &context.search_dirs)
        .map_err(|_| DiagnosticCode::ExecutableUnavailable)?;
    let version = checked_version(AgentId::Codex, &executable, context)
        .await
        .ok_or(DiagnosticCode::VersionUnavailable)?;
    if !version_supported(AgentId::Codex, &version) {
        return Err(DiagnosticCode::UnsupportedVersion);
    }
    if !checked_help(AgentId::Codex, &executable, context).await {
        return Err(DiagnosticCode::AuthUnavailable);
    }
    let mut spec = probe_spec(&executable, context, &[]);
    spec.args = crate::codex_rpc::isolated_args();
    spec.timeout = std::time::Duration::from_secs(15);
    let observation = crate::codex_rpc::read_account(spec, true)
        .await
        .map_err(|_| DiagnosticCode::AuthUnavailable)?;
    if let Some(Err(error)) = &observation.quota {
        return Err(match error {
            crate::process::ProbeError::Timeout => DiagnosticCode::ProbeTimeout,
            crate::process::ProbeError::OutputLimit => DiagnosticCode::ProbeOutputLimit,
            _ => DiagnosticCode::QuotaUnavailable,
        });
    }
    Ok(observation)
}
pub fn resolve_native(id: AgentId, dirs: &[PathBuf]) -> Result<PathBuf, DiscoveryError> {
    let basename = match id {
        AgentId::Claude => "claude",
        AgentId::Codex => "codex",
    };
    let mut unavailable = false;
    for dir in dirs.iter().filter(|dir| dir.is_absolute()) {
        let candidate = dir.join(format!("{basename}.exe"));
        if candidate.exists() {
            unavailable = true;
            if let Ok(canonical) = std::fs::canonicalize(&candidate) {
                if is_native_file(&canonical) {
                    return Ok(canonical);
                }
            }
        }
        unavailable |= dir.join(format!("{basename}.cmd")).exists()
            || dir.join(format!("{basename}.bat")).exists();
    }
    Err(if unavailable {
        DiscoveryError::Unavailable
    } else {
        DiscoveryError::NotFound
    })
}

pub fn parse_version(id: AgentId, output: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(output).ok()?.trim();
    let candidate = match id {
        AgentId::Codex => text.strip_prefix("codex-cli ")?.split_whitespace().next()?,
        AgentId::Claude => text.split_whitespace().next()?,
    };
    valid_version(candidate).then(|| candidate.to_owned())
}

pub fn valid_version(version: &str) -> bool {
    if version.len() > 96
        || !version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
    {
        return false;
    }
    let core = match version.split_once('-') {
        Some((core, _)) => core,
        None => version,
    };
    let parts: Vec<_> = core.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()) && p.parse::<u32>().is_ok()
        })
}

pub fn version_supported(id: AgentId, version: &str) -> bool {
    valid_version(version)
        && match id {
            AgentId::Claude => version.starts_with("2.1."),
            AgentId::Codex => {
                version.starts_with("0.162.")
                    || matches!(
                        version,
                        "0.165.0-alpha.1.2" | "0.166.0-alpha.5.1" | "0.167.0-alpha.3.2"
                    )
            }
        }
}

pub fn is_native_file(path: &Path) -> bool {
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        return false;
    }
    let mut magic = [0_u8; 2];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && magic == *b"MZ"
}
