use jentrics_core::discovery::{probe_codex_limits, DiscoveryContext};
use std::{process::ExitCode, time::Instant};

#[tokio::main]
async fn main() -> ExitCode {
    let search_dirs = match jentrics_core::environment::search_dirs() {
        Ok(directories) => directories,
        Err(_) => {
            println!("{{\"provider\":\"codex\",\"status\":\"searchPathUnavailable\"}}");
            return ExitCode::FAILURE;
        }
    };
    let cwd = match tempfile::tempdir() {
        Ok(cwd) => cwd,
        Err(_) => {
            println!("{{\"provider\":\"codex\",\"status\":\"temporaryDirectoryUnavailable\"}}");
            return ExitCode::FAILURE;
        }
    };
    let started = Instant::now();
    let context = DiscoveryContext {
        search_dirs,
        isolated_cwd: cwd.path().into(),
    };
    // Fixed protocol only: no model turn, login, settings write, or transcript access.
    match probe_codex_limits(&context).await {
        Ok(result) => {
            println!(
                "{}",
                serde_json::json!({"provider":"codex","status":"completed","accountAvailable":result.auth_kind.is_some(),"quotaWindows":result.quota.as_ref().and_then(|quota| quota.as_ref().ok()).map(Vec::len),"quotaError":result.quota.as_ref().and_then(|quota| quota.as_ref().err()).map(|error| format!("{error:?}")),"elapsedMs":started.elapsed().as_millis()})
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!(
                "{}",
                serde_json::json!({"provider":"codex","status":format!("{error:?}"),"elapsedMs":started.elapsed().as_millis()})
            );
            ExitCode::FAILURE
        }
    }
}
