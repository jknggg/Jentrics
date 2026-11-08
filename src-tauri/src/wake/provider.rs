use super::{PreparedWake, WakeReason, WakeRunner};
use crate::{
    discovery::{parse_version, resolve_native, version_supported, DiscoveryError},
    model::AgentId,
    process::{run_probe, ProbeError, ProbeSpec},
};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    time::Duration,
};
mod codex;

pub struct NativeWakeRunner {
    search_dirs: Vec<PathBuf>,
}
impl NativeWakeRunner {
    pub fn new(search_dirs: Vec<PathBuf>) -> Self {
        Self { search_dirs }
    }

    async fn qualify(
        &self,
        agent: AgentId,
        cwd: &Path,
    ) -> Result<(PreparedWake, serde_json::Value), WakeReason> {
        let executable = resolve_native(agent, &self.search_dirs).map_err(|error| match error {
            DiscoveryError::NotFound => WakeReason::NotInstalled,
            DiscoveryError::Unavailable => WakeReason::Unsupported,
        })?;
        let output = run_probe(spec(&executable, cwd, &["--version"], 64 * 1024))
            .await
            .map_err(probe_reason)?;
        if output.exit_code != Some(0) {
            return Err(WakeReason::ProviderUnavailable);
        }
        let version = parse_version(agent, &output.stdout).ok_or(WakeReason::Unsupported)?;
        if !version_supported(agent, &version) {
            return Err(WakeReason::Unsupported);
        }
        let args: &[&str] = match agent {
            AgentId::Codex => &["exec", "--help"],
            AgentId::Claude => &["--help"],
        };
        let help = run_probe(spec(&executable, cwd, args, 64 * 1024))
            .await
            .map_err(probe_reason)?;
        if help.exit_code != Some(0) {
            return Err(WakeReason::ProviderUnavailable);
        }
        let help = std::str::from_utf8(&help.stdout).map_err(|_| WakeReason::Unsupported)?;
        // Claude bare excludes OAuth, and safe/restricted retain managed hooks.
        // No native Claude subscription-safe profile is yet qualified.
        if agent != AgentId::Codex || version != "0.165.0-alpha.1.2" {
            return Err(WakeReason::Unsupported);
        }
        for flag in [
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "--strict-config",
            "--json",
            "--skip-git-repo-check",
        ] {
            if !help.split_whitespace().any(|word| word == flag) {
                return Err(WakeReason::Unsupported);
            }
        }
        codex::check_environment()?;
        let catalog = codex::catalog(&executable, cwd).await?;
        codex::check_auth(&executable, cwd).await?;
        codex::require_single_request()?;
        Ok((
            PreparedWake {
                agent_id: agent,
                executable,
                version,
            },
            catalog,
        ))
    }
}
impl WakeRunner for NativeWakeRunner {
    fn prepare(
        &self,
        agent: AgentId,
    ) -> Pin<Box<dyn Future<Output = Result<PreparedWake, WakeReason>> + Send + '_>> {
        Box::pin(async move {
            let cwd = tempfile::Builder::new()
                .prefix("jentrics-wake-")
                .tempdir()
                .map_err(|_| WakeReason::ProviderUnavailable)?;
            let result = self
                .qualify(agent, cwd.path())
                .await
                .map(|(prepared, _)| prepared);
            cwd.close().map_err(|_| WakeReason::ProviderUnavailable)?;
            result
        })
    }
    fn run(
        &self,
        prepared: PreparedWake,
    ) -> Pin<Box<dyn Future<Output = Result<(), WakeReason>> + Send + '_>> {
        Box::pin(async move {
            let cwd = tempfile::Builder::new()
                .prefix("jentrics-wake-")
                .tempdir()
                .map_err(|_| WakeReason::ProviderUnavailable)?;
            let metadata = tempfile::Builder::new()
                .prefix("jentrics-wake-metadata-")
                .tempdir()
                .map_err(|_| WakeReason::ProviderUnavailable)?;
            let result = async {
                let (current, catalog) = self.qualify(prepared.agent_id, cwd.path()).await?;
                if current.executable != prepared.executable || current.version != prepared.version
                {
                    return Err(WakeReason::Unsupported);
                }
                codex::run(&current.executable, cwd.path(), metadata.path(), catalog).await
            }
            .await;
            let cwd_closed = cwd.close();
            let metadata_closed = metadata.close();
            if cwd_closed.is_err() || metadata_closed.is_err() {
                return Err(WakeReason::ProviderUnavailable);
            }
            result
        })
    }
}
fn spec(executable: &Path, cwd: &Path, args: &[&str], limit: usize) -> ProbeSpec {
    ProbeSpec {
        executable: executable.into(),
        cwd: cwd.into(),
        args: args.iter().map(Into::into).collect(),
        timeout: Duration::from_secs(5),
        max_stream_bytes: limit,
    }
}
fn probe_reason(error: ProbeError) -> WakeReason {
    match error {
        ProbeError::Timeout => WakeReason::TimedOut,
        ProbeError::OutputLimit => WakeReason::OutputLimit,
        ProbeError::InvalidExecutable => WakeReason::Unsupported,
        _ => WakeReason::ProviderUnavailable,
    }
}
