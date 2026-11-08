use crate::{
    adapters::codex::{normalize_codex_limits, QuotaWindow},
    process::{read_bounded, spawn_owned, ProbeError, ProbeSpec},
};
use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

pub fn isolated_args() -> Vec<std::ffi::OsString> {
    [
        "app-server",
        "--listen",
        "stdio://",
        "--strict-config",
        "-c",
        "mcp_servers={}",
        "-c",
        "analytics.enabled=false",
        "-c",
        "otel.exporter=\"none\"",
        "-c",
        "otel.metrics_exporter=\"none\"",
        "-c",
        "plugins={}",
        "-c",
        "apps={_default={enabled=false}}",
        "-c",
        "features.plugins=false",
    ]
    .into_iter()
    .map(Into::into)
    .collect()
}

#[derive(Debug)]
pub struct CodexObservation {
    pub auth_kind: Option<String>,
    pub account_is_null: bool,
    pub requires_auth: bool,
    pub quota: Option<Result<Vec<QuotaWindow>, ProbeError>>,
    pub usage:
        Option<Result<crate::codex_usage::CodexAccountUsage, crate::codex_usage::UsageError>>,
}

async fn send(stdin: &mut tokio::process::ChildStdin, value: Value) -> Result<(), ProbeError> {
    stdin
        .write_all(format!("{value}\n").as_bytes())
        .await
        .map_err(|_| ProbeError::ProtocolFailed)
}
async fn response(
    reader: &mut (impl AsyncBufRead + Unpin),
    id: u64,
    consumed: &mut usize,
    limit: usize,
) -> Result<Value, ProbeError> {
    loop {
        let mut line = String::new();
        let count = reader
            .read_line(&mut line)
            .await
            .map_err(|_| ProbeError::ProtocolFailed)?;
        *consumed += count;
        if *consumed > limit {
            return Err(ProbeError::OutputLimit);
        }
        if count == 0 {
            return Err(ProbeError::ProtocolFailed);
        }
        let value: Value = serde_json::from_str(&line).map_err(|_| ProbeError::ProtocolFailed)?;
        if value.get("id").is_none() && value.get("method").is_some() {
            continue;
        }
        if value.get("id").and_then(Value::as_u64) != Some(id) {
            return Err(ProbeError::ProtocolFailed);
        }
        return Ok(value);
    }
}

pub async fn read_account(
    spec: ProbeSpec,
    include_limits: bool,
) -> Result<CodexObservation, ProbeError> {
    read_account_with_usage(spec, include_limits, None).await
}
pub async fn read_account_with_usage(
    spec: ProbeSpec,
    include_limits: bool,
    usage_version: Option<&str>,
) -> Result<CodexObservation, ProbeError> {
    let mut process = spawn_owned(&spec, true)?;
    let stdout = process.child.stdout.take().ok_or(ProbeError::SpawnFailed)?;
    let stderr = process.child.stderr.take().ok_or(ProbeError::SpawnFailed)?;
    let mut stdin = process.child.stdin.take().ok_or(ProbeError::SpawnFailed)?;
    let deadline = tokio::time::Instant::now() + spec.timeout;
    let protocol = async {
        let mut reader = BufReader::new(stdout.take(spec.max_stream_bytes as u64 + 1));
        let mut consumed = 0;
        let account = tokio::time::timeout_at(deadline, async {
            send(&mut stdin, json!({"id":0,"method":"initialize","params":{"clientInfo":{"name":"jentrics","title":"Jentrics","version":"0.1.0"}}})).await?;
            let init = response(&mut reader, 0, &mut consumed, spec.max_stream_bytes).await?;
            if !init.get("result").is_some_and(Value::is_object) { return Err(ProbeError::ProtocolFailed); }
            send(&mut stdin, json!({"method":"initialized","params":{}})).await?;
            send(&mut stdin, json!({"id":1,"method":"account/read","params":{"refreshToken":false}})).await?;
            response(&mut reader, 1, &mut consumed, spec.max_stream_bytes).await
        }).await.map_err(|_| ProbeError::Timeout)??;
        let data = account
            .get("result")
            .and_then(Value::as_object)
            .ok_or(ProbeError::ProtocolFailed)?;
        let requires_auth = data
            .get("requiresOpenaiAuth")
            .and_then(Value::as_bool)
            .ok_or(ProbeError::ProtocolFailed)?;
        let auth_kind = data
            .get("account")
            .and_then(|a| a.get("type"))
            .and_then(Value::as_str)
            .filter(|v| matches!(*v, "chatgpt" | "apiKey" | "amazonBedrock"))
            .map(str::to_owned);
        let account_is_null = data.get("account").is_some_and(Value::is_null);
        let quota = if include_limits {
            Some(
                tokio::time::timeout_at(deadline, async {
                    send(
                        &mut stdin,
                        json!({"id":2,"method":"account/rateLimits/read"}),
                    )
                    .await?;
                    let limits =
                        response(&mut reader, 2, &mut consumed, spec.max_stream_bytes).await?;
                    if limits.get("error").is_some() {
                        return Err(ProbeError::ProtocolFailed);
                    }
                    let payload = limits.get("result").ok_or(ProbeError::ProtocolFailed)?;
                    normalize_codex_limits(payload, crate::state::now_ms())
                        .map_err(|_| ProbeError::ProtocolFailed)
                })
                .await
                .unwrap_or(Err(ProbeError::Timeout)),
            )
        } else {
            None
        };
        let usage = if let Some(version) =
            usage_version.filter(|v| crate::codex_usage::usage_supported(v))
        {
            Some(if auth_kind.as_deref() != Some("chatgpt") {
                Err(crate::codex_usage::UsageError::CapabilityUnavailable)
            } else {
                tokio::time::timeout_at(deadline, async {
                    send(
                        &mut stdin,
                        crate::codex_usage::request(version, 3)
                            .map_err(|_| ProbeError::ProtocolFailed)?,
                    )
                    .await?;
                    response(&mut reader, 3, &mut consumed, spec.max_stream_bytes).await
                })
                .await
                .map_err(|_| crate::codex_usage::UsageError::Unavailable)
                .and_then(|v| v.map_err(|_| crate::codex_usage::UsageError::Unavailable))
                .and_then(|v| crate::codex_usage::normalize_response(&v, crate::state::now_ms()))
            })
        } else {
            None
        };
        drop(stdin);
        Ok(CodexObservation {
            auth_kind,
            account_is_null,
            requires_auth,
            quota,
            usage,
        })
    };
    tokio::pin!(protocol);
    // Drain stderr concurrently, but never wait for EOF from a long-lived App Server.
    // Raw provider text is discarded; an overflowing stream is an explicit failure.
    let result = tokio::select! {
        biased;
        stderr_result = read_bounded(stderr, spec.max_stream_bytes) => match stderr_result {
            Err(error) => Err(error),
            Ok(_) => protocol.await,
        },
        result = &mut protocol => result,
    };
    process.terminate().await?;
    result
}
