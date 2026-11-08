use super::{TcpStream, TelemetryError};
use crate::telemetry::MAX_BODY_BYTES;
use std::{
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(super) fn read_request(
    stream: &mut TcpStream,
    stop: &AtomicBool,
) -> Result<Vec<u8>, TelemetryError> {
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|_| TelemetryError::Io)?;
    stream
        .set_write_timeout(Some(Duration::from_millis(250)))
        .map_err(|_| TelemetryError::Io)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let header_end = loop {
        if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err(TelemetryError::Io);
        }
        let count = stream.read(&mut buffer).map_err(|_| TelemetryError::Io)?;
        if count == 0 {
            return Err(TelemetryError::Malformed);
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            if index + 4 > 8192 {
                return Err(TelemetryError::TooLarge);
            }
            break index + 4;
        }
        if bytes.len() > 8192 {
            return Err(TelemetryError::TooLarge);
        }
    };
    let headers =
        std::str::from_utf8(&bytes[..header_end]).map_err(|_| TelemetryError::Malformed)?;
    let mut lines = headers.split("\r\n");
    let first = lines.next().ok_or(TelemetryError::Malformed)?;
    if !["POST /v1/metrics HTTP/1.1", "POST /v1/metrics HTTP/1.0"].contains(&first) {
        return Err(TelemetryError::UnsupportedSignal);
    }
    let mut length = None;
    let mut json = false;
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or(TelemetryError::Malformed)?;
        match name.to_ascii_lowercase().as_str() {
            "content-length" => {
                if length.is_some() {
                    return Err(TelemetryError::Malformed);
                }
                length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| TelemetryError::Malformed)?,
                );
            }
            "content-type" => {
                if json {
                    return Err(TelemetryError::Malformed);
                }
                json = value.trim().split(';').next() == Some("application/json");
                if !json {
                    return Err(TelemetryError::UnsupportedShape);
                }
            }
            "transfer-encoding" | "content-encoding" | "origin" | "expect" => {
                return Err(TelemetryError::UnsupportedShape)
            }
            _ => (),
        }
    }
    if !json {
        return Err(TelemetryError::UnsupportedShape);
    }
    let length = length.ok_or(TelemetryError::Malformed)?;
    if length > MAX_BODY_BYTES {
        return Err(TelemetryError::TooLarge);
    }
    let mut body = bytes[header_end..].to_vec();
    if body.len() > length {
        return Err(TelemetryError::Malformed);
    }
    while body.len() < length {
        if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err(TelemetryError::Io);
        }
        let remaining = (length - body.len()).min(buffer.len());
        let count = stream
            .read(&mut buffer[..remaining])
            .map_err(|_| TelemetryError::Io)?;
        if count == 0 {
            return Err(TelemetryError::Malformed);
        }
        body.extend_from_slice(&buffer[..count]);
    }
    Ok(body)
}

pub(super) fn respond(stream: &mut TcpStream, status: u16) -> Result<(), TelemetryError> {
    let reason = match status {
        200 => "OK",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        409 => "Conflict",
        503 => "Service Unavailable",
        _ => "Bad Request",
    };
    let response = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}");
    stream
        .write_all(response.as_bytes())
        .map_err(|_| TelemetryError::Io)
}
