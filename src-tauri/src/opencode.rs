use crate::process::{ProbeOutput, ProbeSpec};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CANDIDATE_VERSION: &str = "2.2.2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeGate {
    UnsupportedVersion,
    StructuredOutputUnavailable,
    NativeIsolationUnqualified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Qualification {
    pub version: String,
    pub gate: NativeGate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenCodeError {
    VersionUnavailable,
    ProbeFailed,
    ProbeTimeout,
    ProbeOutputLimit,
    Malformed,
    TooLarge,
    OutOfRange,
    UnsupportedVersion,
    NativeIsolationUnqualified,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatsSnapshot {
    pub schema_version: u8,
    pub source_profile: String,
    pub observed_ms: i64,
    pub range_from_ms: i64,
    pub range_to_ms: i64,
    pub sessions: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub estimated_cost_usd: f64,
}

pub fn qualify(
    version: &ProbeOutput,
    help: Option<&ProbeOutput>,
) -> Result<Qualification, OpenCodeError> {
    if version.exit_code != Some(0) || version.stdout.len() > 64 {
        return Err(OpenCodeError::VersionUnavailable);
    }
    let text = std::str::from_utf8(&version.stdout)
        .map_err(|_| OpenCodeError::VersionUnavailable)?
        .trim();
    let version = text.strip_prefix("opencode ").unwrap_or(text);
    let components: Vec<_> = version.split('.').collect();
    if components.len() != 3
        || components.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || part.parse::<u32>().is_err()
        })
    {
        return Err(OpenCodeError::VersionUnavailable);
    }
    let gate = if version != CANDIDATE_VERSION {
        NativeGate::UnsupportedVersion
    } else if !help.is_some_and(|output| {
        output.exit_code == Some(0)
            && output.stdout.len() <= 65_536
            && std::str::from_utf8(&output.stdout)
                .is_ok_and(|text| text.split_whitespace().any(|word| word == "--json"))
    }) {
        NativeGate::StructuredOutputUnavailable
    } else {
        NativeGate::NativeIsolationUnqualified
    };
    Ok(Qualification {
        version: version.to_owned(),
        gate,
    })
}

/// Fixture/source-qualified projection only, not proof of native compatibility.
pub fn parse_stats(
    version: &str,
    input: &[u8],
    observed_ms: i64,
) -> Result<StatsSnapshot, OpenCodeError> {
    if version != CANDIDATE_VERSION {
        return Err(OpenCodeError::UnsupportedVersion);
    }
    if input.len() > 65_536 {
        return Err(OpenCodeError::TooLarge);
    }
    if !(0..=8_640_000_000_000_000).contains(&observed_ms) {
        return Err(OpenCodeError::OutOfRange);
    }
    let payload: serde_json::Value =
        serde_json::from_slice(input).map_err(|_| OpenCodeError::Malformed)?;
    let range = payload.get("range").ok_or(OpenCodeError::Malformed)?;
    let range_from_ms = count(range.get("from"))? as i64;
    let range_to_ms = count(range.get("to"))? as i64;
    if range_from_ms > range_to_ms || range_to_ms > 8_640_000_000_000_000 {
        return Err(OpenCodeError::OutOfRange);
    }
    let tokens = payload.get("tokens").ok_or(OpenCodeError::Malformed)?;
    let cache = tokens.get("cache").ok_or(OpenCodeError::Malformed)?;
    let estimated_cost_usd = payload
        .get("cost")
        .and_then(serde_json::Value::as_f64)
        .ok_or(OpenCodeError::Malformed)?;
    if !estimated_cost_usd.is_finite() || estimated_cost_usd < 0.0 {
        return Err(OpenCodeError::OutOfRange);
    }
    Ok(StatsSnapshot {
        schema_version: 1,
        source_profile: "opencodeStatsV2SourceCandidate222".into(),
        observed_ms,
        range_from_ms,
        range_to_ms,
        sessions: count(payload.get("sessions"))?,
        input_tokens: count(tokens.get("input"))?,
        output_tokens: count(tokens.get("output"))?,
        reasoning_tokens: count(tokens.get("reasoning"))?,
        cache_read_tokens: count(cache.get("read"))?,
        cache_write_tokens: count(cache.get("write"))?,
        estimated_cost_usd,
    })
}

pub async fn inspect(executable: &Path, cwd: &Path) -> Result<Qualification, OpenCodeError> {
    let spec = |args: &[&str]| ProbeSpec {
        executable: executable.into(),
        cwd: cwd.into(),
        args: args.iter().map(Into::into).collect(),
        timeout: std::time::Duration::from_secs(5),
        max_stream_bytes: 65_536,
    };
    let version = crate::process::run_probe(spec(&["--version"]))
        .await
        .map_err(probe_error)?;
    let initial = qualify(&version, None)?;
    if initial.version != CANDIDATE_VERSION {
        return Ok(initial);
    }
    let help = crate::process::run_probe(spec(&["stats", "--help"]))
        .await
        .map_err(probe_error)?;
    qualify(&version, Some(&help))
}

fn count(value: Option<&serde_json::Value>) -> Result<u64, OpenCodeError> {
    let value = value
        .and_then(serde_json::Value::as_u64)
        .ok_or(OpenCodeError::Malformed)?;
    if value > 9_007_199_254_740_991 {
        return Err(OpenCodeError::OutOfRange);
    }
    Ok(value)
}

fn probe_error(error: crate::process::ProbeError) -> OpenCodeError {
    match error {
        crate::process::ProbeError::Timeout => OpenCodeError::ProbeTimeout,
        crate::process::ProbeError::OutputLimit => OpenCodeError::ProbeOutputLimit,
        _ => OpenCodeError::ProbeFailed,
    }
}

/// Native stats are deliberately closed until startup isolation is qualified.
pub async fn read_native_stats(
    _executable: &Path,
    _cwd: &Path,
    _qualification: &Qualification,
    _observed_ms: i64,
) -> Result<StatsSnapshot, OpenCodeError> {
    Err(OpenCodeError::NativeIsolationUnqualified)
}
