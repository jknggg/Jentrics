use super::{ingress, BridgeError};
use crate::model::{AgentState, DiagnosticCode, QuotaSnapshot, QuotaSource};
use std::path::Path;
pub fn attach_capture(
    agent: &mut AgentState,
    directory: &Path,
    now_ms: i64,
) -> Result<(), BridgeError> {
    match ingress::read_capture(directory, now_ms) {
        Ok(Some(capture)) => {
            agent.capabilities.quotas = true;
            agent.quota = Some(QuotaSnapshot {
                source: QuotaSource::ClaudeStatusLine,
                observed_ms: capture.observed_ms,
                diagnostic_code: None,
                windows: capture
                    .windows
                    .into_iter()
                    .map(|v| crate::adapters::codex::QuotaWindow {
                        bucket_id: v.bucket_id,
                        window_minutes: Some(v.window_minutes),
                        used_percent: v.used_percent,
                        resets_at_ms: v.resets_at_ms,
                        observed_ms: capture.observed_ms,
                    })
                    .collect(),
            });
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(error) => {
            agent.capabilities.quotas = true;
            agent.quota = Some(QuotaSnapshot {
                source: QuotaSource::ClaudeStatusLine,
                observed_ms: now_ms,
                windows: vec![],
                diagnostic_code: Some(DiagnosticCode::QuotaUnavailable),
            });
            Err(error)
        }
    }
}
/// Runs before the desktop runtime; consumes stdin once and emits no private input.
pub fn capture_cli(args: &[std::ffi::OsString]) -> Option<i32> {
    if args.get(1).is_none_or(|v| v != "--capture-dir") {
        return None;
    }
    let result = (|| {
        if args.len() != 3 {
            return Err(BridgeError::Malformed);
        }
        let directory = Path::new(&args[2]);
        if !directory.is_absolute() {
            return Err(BridgeError::Malformed);
        }
        let now = crate::environment::timestamp_ms(std::time::SystemTime::now())
            .map_err(|_| BridgeError::ClockChanged)?;
        let capture = ingress::capture_stdin(std::io::stdin().lock(), directory, now)?;
        let labels: Vec<_> = capture
            .windows
            .iter()
            .map(|w| {
                format!(
                    "{}: {}",
                    if w.window_minutes == 300 { "5h" } else { "7d" },
                    w.used_percent
                        .map_or_else(|| "unknown".into(), |v| format!("{v}% used"))
                )
            })
            .collect();
        if labels.is_empty() {
            println!("Jentrics: quota not reported");
        } else {
            println!("Jentrics: {}", labels.join(" · "));
        }
        Ok(())
    })();
    Some(match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Jentrics quota capture failed: {error:?}");
            1
        }
    })
}
