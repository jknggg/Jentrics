use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod ingress;
pub mod setup;

pub const MAX_CAPTURE_BYTES: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeError {
    TooLarge,
    Malformed,
    OutOfRange,
    Io,
    UnsupportedSchema,
    ClockChanged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub schema_version: u8,
    pub observed_ms: i64,
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Window {
    pub bucket_id: String,
    pub window_minutes: i64,
    pub used_percent: Option<f64>,
    pub resets_at_ms: Option<i64>,
}

pub fn parse_status_line(input: &[u8], observed_ms: i64) -> Result<Capture, BridgeError> {
    if input.len() > MAX_CAPTURE_BYTES {
        return Err(BridgeError::TooLarge);
    }
    if !(0..=8_640_000_000_000_000).contains(&observed_ms) {
        return Err(BridgeError::OutOfRange);
    }
    let payload: Value = serde_json::from_slice(input).map_err(|_| BridgeError::Malformed)?;
    let fields = payload.as_object().ok_or(BridgeError::Malformed)?;
    let mut windows = Vec::new();
    if let Some(limits) = fields.get("rate_limits").filter(|v| !v.is_null()) {
        let limits = limits.as_object().ok_or(BridgeError::Malformed)?;
        for (name, minutes) in [("five_hour", 300), ("seven_day", 10_080)] {
            let Some(window) = limits.get(name).filter(|v| !v.is_null()) else {
                continue;
            };
            let window = window.as_object().ok_or(BridgeError::Malformed)?;
            let used_percent = window
                .get("used_percentage")
                .filter(|v| !v.is_null())
                .map(|v| v.as_f64().ok_or(BridgeError::Malformed))
                .transpose()?;
            let resets_at_ms = window
                .get("resets_at")
                .filter(|v| !v.is_null())
                .map(|v| {
                    v.as_i64()
                        .ok_or(BridgeError::Malformed)?
                        .checked_mul(1000)
                        .ok_or(BridgeError::OutOfRange)
                })
                .transpose()?;
            let window = Window {
                bucket_id: format!("claude:{name}"),
                window_minutes: minutes,
                used_percent,
                resets_at_ms,
            };
            validate_window(&window)?;
            windows.push(window);
        }
    }
    Ok(Capture {
        schema_version: 1,
        observed_ms,
        windows,
    })
}

pub(crate) fn validate_window(window: &Window) -> Result<(), BridgeError> {
    if !matches!(
        (window.bucket_id.as_str(), window.window_minutes),
        ("claude:five_hour", 300) | ("claude:seven_day", 10_080)
    ) || window
        .used_percent
        .is_some_and(|v| !v.is_finite() || !(0.0..=100.0).contains(&v))
        || window
            .resets_at_ms
            .is_some_and(|v| !(0..=8_640_000_000_000_000).contains(&v))
    {
        return Err(BridgeError::OutOfRange);
    }
    Ok(())
}
pub mod control;
pub mod runtime;
