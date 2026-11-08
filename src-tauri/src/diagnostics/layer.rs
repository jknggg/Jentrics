use super::{EventCode, Health};
use serde::Serialize;
use std::{
    io::Write,
    sync::{atomic::Ordering, Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::{
    field::{Field, Visit},
    Event, Subscriber,
};
use tracing_appender::non_blocking::NonBlocking;
use tracing_subscriber::{layer::Context, Layer};

pub(super) struct FixedLayer {
    pub output: Mutex<NonBlocking>,
    pub health: Arc<Health>,
}
#[derive(Default)]
struct Fields {
    code: Option<&'static str>,
    duration_ms: Option<u64>,
    count: Option<u64>,
    rejected: bool,
}
impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() != "code" {
            self.rejected = true;
            return;
        }
        self.code = [
            EventCode::Startup,
            EventCode::TrayReady,
            EventCode::RefreshCompleted,
            EventCode::MonitorChanged,
            EventCode::TrayUpdateFailed,
            EventCode::Shutdown,
        ]
        .into_iter()
        .map(EventCode::as_str)
        .find(|code| *code == value);
        if self.code.is_none() {
            self.rejected = true;
        }
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "duration_ms" => self.duration_ms = Some(value),
            "count" => self.count = Some(value),
            _ => self.rejected = true,
        }
    }
    fn record_debug(&mut self, _: &Field, _: &dyn std::fmt::Debug) {
        self.rejected = true;
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    schema_version: u8,
    timestamp_ms: u64,
    code: &'static str,
    duration_ms: u64,
    count: u64,
}
impl<S: Subscriber> Layer<S> for FixedLayer {
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _: Context<'_, S>) -> bool {
        metadata.is_event() && metadata.target() == "jentrics::diagnostics"
    }
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() != "jentrics::diagnostics" {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if fields.rejected {
            return;
        }
        let (Some(code), Some(duration_ms), Some(count)) =
            (fields.code, fields.duration_ms, fields.count)
        else {
            return;
        };
        let Ok(timestamp) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            self.health.failure.store(2, Ordering::Relaxed);
            return;
        };
        let Ok(timestamp_ms) = u64::try_from(timestamp.as_millis()) else {
            self.health.failure.store(2, Ordering::Relaxed);
            return;
        };
        let record = Record {
            schema_version: 1,
            timestamp_ms,
            code,
            duration_ms,
            count,
        };
        let Ok(mut bytes) = serde_json::to_vec(&record) else {
            self.health.failure.store(2, Ordering::Relaxed);
            return;
        };
        bytes.push(b'\n');
        self.health.accepted.fetch_add(1, Ordering::Relaxed);
        if self
            .output
            .lock()
            .expect("diagnostic output lock poisoned")
            .write_all(&bytes)
            .is_err()
        {
            self.health.failure.store(2, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    #[test]
    fn tray_registration_marker_is_timestamped_without_extra_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let diagnostics = Diagnostics::open(directory.path());
        tracing::dispatcher::with_default(diagnostics.dispatch.as_ref().unwrap(), || {
            tracing::event!(target:"jentrics::diagnostics",tracing::Level::INFO,code="trayReady",duration_ms=0u64,count=1u64);
        });
        diagnostics.shutdown();
        let text = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect::<String>();
        assert_eq!(text.lines().count(), 1);
        let record: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(record.as_object().unwrap().len(), 5);
        assert_eq!(record["code"], "trayReady");
        assert!(record["timestampMs"].as_u64().unwrap() > 0);
        assert_eq!(record["count"], 1);
    }
    #[test]
    fn third_party_messages_spans_and_extra_private_fields_are_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let diagnostics = Diagnostics::open(dir.path());
        tracing::dispatcher::with_default(diagnostics.dispatch.as_ref().unwrap(), || {
            let span = tracing::info_span!("private span", repo = "secret repository");
            let _entered = span.enter();
            tracing::info!(target:"thirdparty", "secret account");
            tracing::event!(target:"jentrics::diagnostics",tracing::Level::INFO,code="startup",duration_ms=0u64,count=0u64,prompt="private prompt");
            tracing::event!(target:"jentrics::diagnostics",tracing::Level::INFO,code="unknown secret",duration_ms=0u64,count=0u64);
        });
        diagnostics.record(EventCode::Startup, 0, 1);
        diagnostics.shutdown();
        let text = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect::<String>();
        assert_eq!(text.lines().count(), 1);
        assert!(!text.contains("secret"));
        assert!(!text.contains("private"));
        assert!(!text.contains("repo"));
    }
}
