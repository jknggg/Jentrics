use super::TelemetryError;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Hash)]
pub(super) struct RequestIdentity {
    pub outcome: &'static str,
    status: String,
    error_category: Option<String>,
}

fn category(text: &str) -> Result<String, TelemetryError> {
    if text.is_empty()
        || text.len() > 128
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(TelemetryError::UnsupportedShape);
    }
    Ok(text.to_owned())
}

/// Gemini's numeric request counter distinguishes error events by error_type;
/// status_code is a number or string, with documented "ok"/"error" fallbacks.
/// Categories contribute only to the opaque series fingerprint: separate
/// cumulative counters must not be merged. No raw status/category is persisted.
pub(super) fn classify(attrs: &BTreeMap<String, Value>) -> Result<RequestIdentity, TelemetryError> {
    let value = attrs
        .get("status_code")
        .and_then(Value::as_object)
        .filter(|object| object.len() == 1)
        .ok_or(TelemetryError::Malformed)?;
    let status = if let Some(value) = value.get("intValue") {
        let integer = value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
            .ok_or(TelemetryError::Malformed)?;
        if integer != 0 && !(100..=599).contains(&integer) {
            return Err(TelemetryError::OutOfRange);
        }
        integer.to_string()
    } else {
        let text = value
            .get("stringValue")
            .and_then(Value::as_str)
            .ok_or(TelemetryError::Malformed)?;
        if text.bytes().all(|byte| byte.is_ascii_digit()) && !text.is_empty() {
            let integer = text
                .parse::<u64>()
                .map_err(|_| TelemetryError::OutOfRange)?;
            if integer != 0 && !(100..=599).contains(&integer) {
                return Err(TelemetryError::OutOfRange);
            }
            integer.to_string()
        } else {
            category(text)?
        }
    };
    let error_category = attrs
        .get("error_type")
        .map(|value| {
            let object = value
                .as_object()
                .filter(|object| object.len() == 1)
                .ok_or(TelemetryError::Malformed)?;
            category(
                object
                    .get("stringValue")
                    .and_then(Value::as_str)
                    .ok_or(TelemetryError::Malformed)?,
            )
        })
        .transpose()?;
    let outcome = if error_category.is_some()
        || status == "error"
        || status
            .parse::<u16>()
            .is_ok_and(|value| (400..=599).contains(&value))
    {
        "error"
    } else if status == "ok"
        || status
            .parse::<u16>()
            .is_ok_and(|value| (200..=299).contains(&value))
    {
        "success"
    } else {
        "unknown"
    };
    Ok(RequestIdentity {
        outcome,
        status,
        error_category,
    })
}
