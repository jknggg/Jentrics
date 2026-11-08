// Bundled, read-only setup instructions: enabling the receiver never edits a provider.
export const telemetrySetupGuide = `Review the installed provider version and effective managed, project and user settings first. Preserve an existing telemetry destination. Apply changes deliberately to one shell session; remove them separately to stop provider exports.

CLAUDE CODE · reviewed PowerShell session
$env:CLAUDE_CODE_ENABLE_TELEMETRY = '1'
$env:OTEL_METRICS_EXPORTER = 'otlp'
$env:OTEL_EXPORTER_OTLP_METRICS_PROTOCOL = 'http/json'
$env:OTEL_EXPORTER_OTLP_METRICS_ENDPOINT = 'http://127.0.0.1:43190/v1/metrics'
$env:OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE = 'delta'
$env:OTEL_LOGS_EXPORTER = 'none'
$env:OTEL_TRACES_EXPORTER = 'none'
$env:OTEL_METRICS_INCLUDE_SESSION_ID = 'true'
$env:OTEL_METRICS_INCLUDE_ACCOUNT_UUID = 'false'
$env:OTEL_METRICS_INCLUDE_RESOURCE_ATTRIBUTES = 'false'
$env:OTEL_METRICS_INCLUDE_REPOSITORY = 'false'
$env:OTEL_LOG_USER_PROMPTS = '0'
$env:OTEL_LOG_TOOL_DETAILS = '0'

GEMINI CLI · source-qualified HTTP/JSON profile
$env:GEMINI_TELEMETRY_ENABLED = 'true'
$env:GEMINI_TELEMETRY_TARGET = 'local'
$env:GEMINI_TELEMETRY_OTLP_PROTOCOL = 'http'
$env:GEMINI_TELEMETRY_OTLP_ENDPOINT = 'http://127.0.0.1:43190'
$env:GEMINI_TELEMETRY_LOG_PROMPTS = 'false'
$env:GEMINI_TELEMETRY_TRACES_ENABLED = 'false'

Do not configure an output file containing raw telemetry. Review separate log exporters. Jentrics rejects logs, traces, protobuf, compressed/chunked requests, unknown shapes, custom dimensions and named agent/skill/plugin/MCP attribution. Rejected or absent data is not zero usage. Check baseline, duplicate, ignored and rejected counters after configuring exports.

Actual Claude and Gemini session export remains unverified on this machine. These suggestions do not override managed settings or establish compatibility with every version. No model request is needed to review settings; Jentrics does not start a session.

Claude subscription limits use the separate reviewed status-line workflow. Numeric token telemetry cannot substitute for reported quota windows. Cumulative baselines restart with the collector; missing coverage is not reconstructed. Disable the local collector to close its listener, and remove provider changes separately. Clearing local telemetry does not change provider settings.`;
