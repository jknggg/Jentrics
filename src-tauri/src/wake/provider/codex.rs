use super::{probe_reason, spec};
use crate::{
    process::{run_probe, ProbeOutput},
    wake::{WakeReason, WAKE_PROMPT},
};
use serde_json::Value;
use std::{ffi::OsString, path::Path, time::Duration};

fn configs() -> Vec<OsString> {
    let mut args = Vec::new();
    for feature in [
        "shell_tool",
        "multi_agent",
        "multi_agent_v2",
        "hooks",
        "plugins",
        "memories",
        "apps",
        "code_mode",
        "code_mode_host",
        "computer_use",
        "browser_use",
        "browser_use_external",
        "image_generation",
        "view_image",
        "goals",
        "sleep_tool",
        "tool_suggest",
        "remote_plugin",
        "shell_snapshot",
        "workspace_dependencies",
        "skill_search",
        "skill_mcp_dependency_install",
        "current_time_reminder",
    ] {
        args.push("-c".into());
        args.push(format!("features.{feature}=false").into());
    }
    for setting in [
        "agents.enabled=false",
        "features.skip_host_skill_discovery=true",
        "tools.experimental_request_user_input.enabled=false",
        "tools.update_plan.enabled=false",
        "web_search=\"disabled\"",
        "mcp_servers={}",
        "project_doc_max_bytes=0",
        "model_provider=\"openai\"",
        "analytics.enabled=false",
        "otel.exporter=\"none\"",
        "otel.metrics_exporter=\"none\"",
    ] {
        args.push("-c".into());
        args.push(setting.into());
    }
    args
}

pub(super) fn check_environment() -> Result<(), WakeReason> {
    // exec explicitly honors CODEX_API_KEY independently of the saved login.
    // Reject inherited alternate auth/routes instead of silently changing billing.
    if std::env::vars_os().any(|(name, value)| {
        let name = name.to_string_lossy().to_ascii_uppercase();
        !value.is_empty()
            && (name.starts_with("OPENAI_")
                || matches!(
                    name.as_str(),
                    "CODEX_API_KEY"
                        | "CODEX_ACCESS_TOKEN"
                        | "CODEX_MODEL_PROVIDER"
                        | "CODEX_BASE_URL"
                        | "CHATGPT_BASE_URL"
                        | "CODEX_MANAGED_CONFIG_TOML"
                ))
    }) {
        return Err(WakeReason::SubscriptionRequired);
    }
    Ok(())
}

pub(super) fn require_single_request() -> Result<(), WakeReason> {
    // Qualified 0.165 CLI reserves its built-in OpenAI provider. Its HTTP/SSE
    // retries cannot be set to zero without changing the provider. Keep paid
    // execution unavailable until the approved no-retry contract can be enforced.
    Err(WakeReason::RetryPolicyUnsupported)
}

pub(super) async fn catalog(executable: &Path, cwd: &Path) -> Result<Value, WakeReason> {
    let mut input = spec(executable, cwd, &[], 1024 * 1024);
    input.args = configs();
    input
        .args
        .extend(["debug", "models", "--bundled"].map(OsString::from));
    let output = run_probe(input).await.map_err(probe_reason)?;
    if output.exit_code != Some(0) {
        return Err(WakeReason::Unsupported);
    }
    let mut catalog: Value =
        serde_json::from_slice(&output.stdout).map_err(|_| WakeReason::Unsupported)?;
    let models = catalog
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .ok_or(WakeReason::Unsupported)?;
    if models.is_empty() || models.len() > 256 {
        return Err(WakeReason::Unsupported);
    }
    // Preserve the entire shipped catalog and provider default selection. Only the
    // descriptors that re-enable tools despite CLI feature flags are constrained.
    for model in models {
        let model = model.as_object_mut().ok_or(WakeReason::Unsupported)?;
        if !model
            .get("slug")
            .and_then(Value::as_str)
            .is_some_and(|slug| !slug.is_empty())
        {
            return Err(WakeReason::Unsupported);
        }
        for field in ["tool_mode", "apply_patch_tool_type", "multi_agent_version"] {
            if !model
                .get(field)
                .is_some_and(|v| v.is_string() || v.is_null())
            {
                return Err(WakeReason::Unsupported);
            }
            model.insert(field.into(), Value::Null);
        }
        if !model
            .get("experimental_supported_tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| tools.iter().all(Value::is_string))
        {
            return Err(WakeReason::Unsupported);
        }
        model.insert("experimental_supported_tools".into(), Value::Array(vec![]));
    }
    Ok(catalog)
}

pub(super) async fn check_auth(executable: &Path, cwd: &Path) -> Result<(), WakeReason> {
    let mut input = spec(executable, cwd, &[], 64 * 1024);
    input.args = configs();
    input.args.extend(["login", "status"].map(OsString::from));
    let output = run_probe(input).await.map_err(probe_reason)?;
    let stdout =
        std::str::from_utf8(&output.stdout).map_err(|_| WakeReason::ProviderUnavailable)?;
    let stderr =
        std::str::from_utf8(&output.stderr).map_err(|_| WakeReason::ProviderUnavailable)?;
    let lines: Vec<_> = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .collect();
    if output.exit_code == Some(0) && lines.contains(&"Logged in using ChatGPT") {
        return Ok(());
    }
    if output.exit_code == Some(1) && lines.contains(&"Not logged in") {
        return Err(WakeReason::SignInRequired);
    }
    if lines
        .iter()
        .any(|line| line.starts_with("Logged in using an API key"))
    {
        return Err(WakeReason::SubscriptionRequired);
    }
    Err(WakeReason::ProviderUnavailable)
}

pub(super) async fn run(
    executable: &Path,
    cwd: &Path,
    metadata: &Path,
    catalog: Value,
) -> Result<(), WakeReason> {
    check_environment()?;
    let path = metadata.join("catalog.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&catalog).map_err(|_| WakeReason::Unsupported)?,
    )
    .map_err(|_| WakeReason::ProviderUnavailable)?;
    let mut input = spec(executable, cwd, &[], 128 * 1024);
    input.timeout = Duration::from_secs(90);
    input.args = [
        "--no-daemon",
        "exec",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--strict-config",
        "--json",
        "--skip-git-repo-check",
        "--sandbox",
        "read-only",
    ]
    .map(OsString::from)
    .to_vec();
    input.args.extend(configs());
    input.args.extend([
        OsString::from("-c"),
        format!(
            "model_catalog_json={}",
            serde_json::to_string(&path).map_err(|_| WakeReason::Unsupported)?
        )
        .into(),
        WAKE_PROMPT.into(),
    ]);
    let output = run_probe(input)
        .await
        .map_err(|error| match probe_reason(error) {
            WakeReason::ProviderUnavailable => WakeReason::RequestFailed,
            reason => reason,
        })?;
    completed(&output)
}

fn completed(output: &ProbeOutput) -> Result<(), WakeReason> {
    if output.exit_code != Some(0) {
        return Err(WakeReason::RequestFailed);
    }
    let mut thread = false;
    let mut started = false;
    let mut answer = false;
    let mut done = false;
    let text = std::str::from_utf8(&output.stdout).map_err(|_| WakeReason::RequestFailed)?;
    for line in text.lines() {
        let event: Value = serde_json::from_str(line).map_err(|_| WakeReason::RequestFailed)?;
        if done {
            return Err(WakeReason::RequestFailed);
        }
        match event.get("type").and_then(Value::as_str) {
            Some("thread.started")
                if !thread
                    && !started
                    && event
                        .get("thread_id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| !id.is_empty()) =>
            {
                thread = true
            }
            Some("turn.started") if thread && !started => started = true,
            Some("item.started" | "item.updated" | "item.completed") if started => {
                let item = event.get("item").ok_or(WakeReason::RequestFailed)?;
                match item.get("type").and_then(Value::as_str) {
                    Some("reasoning") => {}
                    Some("agent_message") => {
                        if event["type"] == "item.completed" {
                            if !item
                                .get("text")
                                .and_then(Value::as_str)
                                .is_some_and(|text| !text.trim().is_empty())
                            {
                                return Err(WakeReason::RequestFailed);
                            }
                            answer = true;
                        }
                    }
                    _ => return Err(WakeReason::RequestFailed),
                }
            }
            Some("turn.completed") if started && answer => {
                let usage = event.get("usage").ok_or(WakeReason::RequestFailed)?;
                if !["input_tokens", "output_tokens"]
                    .iter()
                    .all(|key| usage.get(key).and_then(Value::as_u64).is_some())
                {
                    return Err(WakeReason::RequestFailed);
                }
                done = true;
            }
            _ => return Err(WakeReason::RequestFailed),
        }
    }
    if done {
        Ok(())
    } else {
        Err(WakeReason::RequestFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn output(text: &str, exit: i32) -> ProbeOutput {
        ProbeOutput {
            exit_code: Some(exit),
            stdout: text.as_bytes().to_vec(),
            stderr: vec![],
        }
    }
    const SUCCESS:&str=concat!(
        "{\"type\":\"thread.started\",\"thread_id\":\"fixture-thread\"}\n",
        "{\"type\":\"turn.started\"}\n",
        "{\"type\":\"item.completed\",\"item\":{\"id\":\"answer\",\"type\":\"agent_message\",\"text\":\"hello\"}}\n",
        "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":10,\"cached_input_tokens\":0,\"output_tokens\":1}}\n");
    #[test]
    fn dormant_completion_validator_requires_real_terminal_success() {
        assert_eq!(completed(&output(SUCCESS, 0)), Ok(()));
        assert_eq!(
            completed(&output(SUCCESS, 1)),
            Err(WakeReason::RequestFailed)
        );
        assert_eq!(
            completed(&output(
                &SUCCESS.replace("turn.completed", "turn.failed"),
                0
            )),
            Err(WakeReason::RequestFailed)
        );
        assert_eq!(
            completed(&output("not-json\n", 0)),
            Err(WakeReason::RequestFailed)
        );
        let no_terminal = SUCCESS.lines().take(3).collect::<Vec<_>>().join("\n");
        assert_eq!(
            completed(&output(&no_terminal, 0)),
            Err(WakeReason::RequestFailed)
        );
    }
    #[test]
    fn dormant_completion_validator_rejects_every_nontext_tool_event() {
        for tool in [
            "command_execution",
            "file_change",
            "mcp_tool_call",
            "web_search",
            "collab_tool_call",
            "future_tool",
        ] {
            let events = SUCCESS.replace("agent_message", tool);
            assert_eq!(
                completed(&output(&events, 0)),
                Err(WakeReason::RequestFailed)
            );
        }
        assert_eq!(
            completed(&output(&format!("{SUCCESS}{SUCCESS}"), 0)),
            Err(WakeReason::RequestFailed)
        );
    }
}
