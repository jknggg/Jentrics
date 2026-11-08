// One bounded sanitized file is the native-helper/app handoff. Temporary siblings
// are never read; atomic replacement also handles rotation without append offsets.
use super::{BridgeError, Capture, MAX_CAPTURE_BYTES};
use std::{
    io::{Read, Write},
    path::Path,
};

pub const CAPTURE_FILE: &str = "latest.json";

pub fn capture_stdin(
    reader: impl Read,
    directory: &Path,
    observed_ms: i64,
) -> Result<Capture, BridgeError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_CAPTURE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| BridgeError::Io)?;
    let capture = super::parse_status_line(&bytes, observed_ms)?;
    write_capture(directory, &capture)?;
    Ok(capture)
}

pub fn write_capture(directory: &Path, capture: &Capture) -> Result<(), BridgeError> {
    validate_capture(capture)?;
    let bytes = serde_json::to_vec(capture).map_err(|_| BridgeError::Malformed)?;
    if bytes.len() > MAX_CAPTURE_BYTES {
        return Err(BridgeError::TooLarge);
    }
    std::fs::create_dir_all(directory).map_err(|_| BridgeError::Io)?;
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|_| BridgeError::Io)?;
    file.write_all(&bytes).map_err(|_| BridgeError::Io)?;
    file.as_file().sync_all().map_err(|_| BridgeError::Io)?;
    file.persist(directory.join(CAPTURE_FILE))
        .map_err(|_| BridgeError::Io)?;
    Ok(())
}

pub fn read_capture(directory: &Path, now_ms: i64) -> Result<Option<Capture>, BridgeError> {
    if !(0..=8_640_000_000_000_000).contains(&now_ms) {
        return Err(BridgeError::ClockChanged);
    }
    let file = match std::fs::File::open(directory.join(CAPTURE_FILE)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(BridgeError::Io),
    };
    let mut bytes = Vec::new();
    file.take(MAX_CAPTURE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| BridgeError::Io)?;
    if bytes.len() > MAX_CAPTURE_BYTES {
        return Err(BridgeError::TooLarge);
    }
    let capture: Capture = serde_json::from_slice(&bytes).map_err(|_| BridgeError::Malformed)?;
    validate_capture(&capture)?;
    if now_ms < capture.observed_ms {
        return Err(BridgeError::ClockChanged);
    }
    Ok(Some(capture))
}

fn validate_capture(capture: &Capture) -> Result<(), BridgeError> {
    if capture.schema_version != 1 {
        return Err(BridgeError::UnsupportedSchema);
    }
    if !(0..=8_640_000_000_000_000).contains(&capture.observed_ms) || capture.windows.len() > 2 {
        return Err(BridgeError::OutOfRange);
    }
    for (index, window) in capture.windows.iter().enumerate() {
        super::validate_window(window)?;
        if capture.windows[..index]
            .iter()
            .any(|other| other.bucket_id == window.bucket_id)
        {
            return Err(BridgeError::Malformed);
        }
    }
    Ok(())
}

pub fn is_stale(capture: &Capture, now_ms: i64, max_age_ms: i64) -> Result<bool, BridgeError> {
    if now_ms < capture.observed_ms || max_age_ms < 0 {
        return Err(BridgeError::ClockChanged);
    }
    Ok(now_ms - capture.observed_ms > max_age_ms)
}
