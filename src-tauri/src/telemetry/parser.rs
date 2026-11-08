use super::{Metric, Provider, TelemetryError, MAX_BODY_BYTES};
use serde_json::Value;
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    hash::{Hash, Hasher},
};

#[derive(Clone)]
pub(super) struct Point {
    pub key: u64,
    pub provider: Provider,
    pub metric: Metric,
    pub dimension: Option<String>,
    pub model: Option<String>,
    pub start: u64,
    pub end: u64,
    pub value: f64,
    pub count: Option<u64>,
    pub cumulative: bool,
}

pub(super) fn parse(input: &[u8]) -> Result<(Vec<Point>, usize), TelemetryError> {
    if input.len() > MAX_BODY_BYTES {
        return Err(TelemetryError::TooLarge);
    }
    let payload: Value = serde_json::from_slice(input).map_err(|_| TelemetryError::Malformed)?;
    let object = payload.as_object().ok_or(TelemetryError::Malformed)?;
    if object.keys().any(|key| key != "resourceMetrics") {
        return Err(TelemetryError::UnsupportedSignal);
    }
    let resources = array(&payload, "resourceMetrics")?;
    let mut points = Vec::new();
    let mut ignored = 0;
    for resource in resources {
        let resource_attrs = attributes(resource.get("resource").unwrap_or(&Value::Null))?;
        for scope in array(resource, "scopeMetrics")? {
            for metric in array(scope, "metrics")? {
                let name = metric
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or(TelemetryError::Malformed)?;
                let (provider, kind, unit, histogram) = match name {
                    "claude_code.token.usage" => {
                        (Provider::Claude, Metric::Tokens, "tokens", false)
                    }
                    "claude_code.cost.usage" => (Provider::Claude, Metric::CostUsd, "USD", false),
                    "claude_code.active_time.total" => {
                        (Provider::Claude, Metric::ActiveSeconds, "s", false)
                    }
                    "gemini_cli.token.usage" => (Provider::Gemini, Metric::Tokens, "tokens", false),
                    "gemini_cli.api.request.latency" => {
                        (Provider::Gemini, Metric::RequestLatencyMs, "ms", true)
                    }
                    "gemini_cli.api.request.count" => {
                        (Provider::Gemini, Metric::ApiRequests, "1", false)
                    }
                    _ => {
                        ignored += 1;
                        continue;
                    }
                };
                if let Some(reported) = metric.get("unit") {
                    let reported = reported.as_str().ok_or(TelemetryError::Malformed)?;
                    if reported != unit
                        && !(matches!(kind, Metric::Tokens | Metric::ApiRequests)
                            && reported.is_empty())
                    {
                        return Err(TelemetryError::UnsupportedShape);
                    }
                }
                let data = metric
                    .get(if histogram { "histogram" } else { "sum" })
                    .ok_or(TelemetryError::UnsupportedShape)?;
                if !histogram && data.get("isMonotonic").and_then(Value::as_bool) != Some(true) {
                    return Err(TelemetryError::UnsupportedShape);
                }
                let cumulative = match data.get("aggregationTemporality").and_then(Value::as_u64) {
                    Some(1) => false,
                    Some(2) => true,
                    _ => return Err(TelemetryError::UnsupportedShape),
                };
                for point in array(data, "dataPoints")? {
                    if points.len() == 256 {
                        return Err(TelemetryError::TooLarge);
                    }
                    let attrs = attributes(point)?;
                    for (key, allowed) in [
                        ("query_source", ["main", "subagent", "auxiliary"].as_slice()),
                        ("speed", ["fast"].as_slice()),
                        (
                            "effort",
                            ["low", "medium", "high", "xhigh", "max"].as_slice(),
                        ),
                    ] {
                        if attrs.contains_key(key)
                            && !string_attribute(&attrs, key)
                                .is_some_and(|value| allowed.contains(&value))
                        {
                            return Err(TelemetryError::UnsupportedShape);
                        }
                    }
                    if [
                        "agent.name",
                        "skill.name",
                        "plugin.name",
                        "marketplace.name",
                        "mcp_server.name",
                        "mcp_tool.name",
                        "agent_id",
                        "parent_agent_id",
                    ]
                    .iter()
                    .any(|name| attrs.contains_key(*name) || resource_attrs.contains_key(*name))
                    {
                        return Err(TelemetryError::UnsupportedShape);
                    }
                    if !has_identity(&resource_attrs) && !has_identity(&attrs) {
                        return Err(TelemetryError::IdentityUnavailable);
                    }
                    let request_identity = if kind == Metric::ApiRequests {
                        Some(super::request_count::classify(&attrs)?)
                    } else {
                        None
                    };
                    let dimension = match kind {
                        Metric::ApiRequests => request_identity
                            .as_ref()
                            .map(|identity| identity.outcome.to_owned()),
                        Metric::Tokens => {
                            let value = string_attribute(&attrs, "type")
                                .ok_or(TelemetryError::Malformed)?;
                            let allowed = match provider {
                                Provider::Claude => {
                                    ["input", "output", "cacheRead", "cacheCreation"].as_slice()
                                }
                                Provider::Gemini => {
                                    ["input", "output", "thought", "cache", "tool"].as_slice()
                                }
                            };
                            if !allowed.contains(&value) {
                                return Err(TelemetryError::UnsupportedShape);
                            }
                            Some(value.to_owned())
                        }
                        Metric::ActiveSeconds => {
                            let value = string_attribute(&attrs, "type")
                                .ok_or(TelemetryError::Malformed)?;
                            if !["user", "cli"].contains(&value) {
                                return Err(TelemetryError::UnsupportedShape);
                            }
                            Some(value.to_owned())
                        }
                        _ => None,
                    };
                    if attrs.contains_key("model") && string_attribute(&attrs, "model").is_none() {
                        return Err(TelemetryError::Malformed);
                    }
                    let model = string_attribute(&attrs, "model")
                        .map(|value| {
                            if value.is_empty()
                                || value.len() > 128
                                || !value
                                    .bytes()
                                    .all(|c| c.is_ascii_alphanumeric() || b"._-:".contains(&c))
                            {
                                return Err(TelemetryError::OutOfRange);
                            }
                            Ok(value.to_owned())
                        })
                        .transpose()?;
                    let start = integer(point.get("startTimeUnixNano"))?;
                    let end = integer(point.get("timeUnixNano"))?;
                    if start == 0 || end <= start || end / 1_000_000 > i64::MAX as u64 {
                        return Err(TelemetryError::OutOfRange);
                    }
                    let (value, count) = if histogram {
                        let count = integer(point.get("count"))?;
                        let sum = number(point.get("sum"))?;
                        if count == 0 && sum != 0.0 {
                            return Err(TelemetryError::OutOfRange);
                        }
                        (sum, Some(count))
                    } else {
                        let value = match (point.get("asInt"), point.get("asDouble")) {
                            (Some(value), None) => integer(Some(value))? as f64,
                            (None, Some(value)) => number(Some(value))?,
                            _ => return Err(TelemetryError::Malformed),
                        };
                        if matches!(kind, Metric::Tokens | Metric::ApiRequests)
                            && value.fract() != 0.0
                        {
                            return Err(TelemetryError::OutOfRange);
                        }
                        (value, None)
                    };
                    if value > 9_007_199_254_740_991.0
                        || count.is_some_and(|v| v > 9_007_199_254_740_991)
                    {
                        return Err(TelemetryError::OutOfRange);
                    }
                    // A deterministic diagnostic fingerprint permits persisted
                    // interval dedup. It is not cryptography or an identity proof.
                    // Named attribution is unsupported. Only producer identity,
                    // admitted model/type and bounded request categories enter
                    // the fingerprint. Rust upgrades may change this
                    // hash algorithm: do not claim replay coverage across upgrades.
                    let resource_identity = selected(
                        &resource_attrs,
                        &["session.id", "service.instance.id", "process.pid"],
                    )?;
                    let point_identity = selected(
                        &attrs,
                        &[
                            "session.id",
                            "service.instance.id",
                            "process.pid",
                            "model",
                            "type",
                            "query_source",
                            "speed",
                            "effort",
                        ],
                    )?;
                    let mut hasher = DefaultHasher::new();
                    (name, resource_identity, point_identity).hash(&mut hasher);
                    if let Some(identity) = request_identity {
                        identity.hash(&mut hasher)
                    }
                    let key = hasher.finish();
                    points.push(Point {
                        key,
                        provider,
                        metric: kind,
                        dimension,
                        model,
                        start,
                        end,
                        value,
                        count,
                        cumulative,
                    });
                }
            }
        }
    }
    if points.is_empty() {
        return Err(TelemetryError::UnsupportedMetric);
    }
    Ok((points, ignored))
}

fn selected(attrs: &BTreeMap<String, Value>, keys: &[&str]) -> Result<String, TelemetryError> {
    let selected: BTreeMap<_, _> = keys
        .iter()
        .filter_map(|key| attrs.get(*key).map(|value| (*key, value)))
        .collect();
    serde_json::to_string(&selected).map_err(|_| TelemetryError::Malformed)
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, TelemetryError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or(TelemetryError::Malformed)
}

// Serialize only for transient canonical hashing; never persist these strings.
fn attributes(value: &Value) -> Result<BTreeMap<String, Value>, TelemetryError> {
    let Some(attrs) = value.get("attributes") else {
        return Ok(BTreeMap::new());
    };
    let attrs = attrs.as_array().ok_or(TelemetryError::Malformed)?;
    if attrs.len() > 64 {
        return Err(TelemetryError::TooLarge);
    }
    let mut result = BTreeMap::new();
    for attr in attrs {
        let key = attr
            .get("key")
            .and_then(Value::as_str)
            .ok_or(TelemetryError::Malformed)?;
        let value = attr.get("value").ok_or(TelemetryError::Malformed)?;
        if result.insert(key.to_owned(), value.clone()).is_some() {
            return Err(TelemetryError::Malformed);
        }
    }
    Ok(result)
}

fn string_attribute<'a>(attrs: &'a BTreeMap<String, Value>, name: &str) -> Option<&'a str> {
    attrs.get(name)?.get("stringValue")?.as_str()
}

fn has_identity(attrs: &BTreeMap<String, Value>) -> bool {
    ["session.id", "service.instance.id"].iter().any(|name| {
        string_attribute(attrs, name).is_some_and(|value| !value.is_empty() && value.len() <= 256)
    })
}

fn integer(value: Option<&Value>) -> Result<u64, TelemetryError> {
    let value = value.ok_or(TelemetryError::Malformed)?;
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .ok_or(TelemetryError::Malformed)
}

fn number(value: Option<&Value>) -> Result<f64, TelemetryError> {
    let value = value
        .and_then(Value::as_f64)
        .ok_or(TelemetryError::Malformed)?;
    if !value.is_finite() || value < 0.0 {
        return Err(TelemetryError::OutOfRange);
    }
    Ok(value)
}
