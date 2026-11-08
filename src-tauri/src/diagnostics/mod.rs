//! App-owned diagnostic records. There is deliberately no arbitrary message API.
mod layer;
mod writer;
use serde::Serialize;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, AtomicU8, Ordering},
        Arc, Mutex,
    },
};
use tracing_appender::non_blocking::{ErrorCounter, WorkerGuard};
use tracing_subscriber::prelude::*;

#[derive(Debug, Clone, Copy)]
pub enum EventCode {
    Startup,
    TrayReady,
    RefreshCompleted,
    MonitorChanged,
    TrayUpdateFailed,
    Shutdown,
}
impl EventCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::TrayReady => "trayReady",
            Self::RefreshCompleted => "refreshCompleted",
            Self::MonitorChanged => "monitorChanged",
            Self::TrayUpdateFailed => "trayUpdateFailed",
            Self::Shutdown => "shutdown",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StatusCode {
    NotStarted,
    Healthy,
    InitializationUnavailable,
    WriteUnavailable,
    CapacityReached,
    QueueDropped,
    ShutdownIncomplete,
    Stopped,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticStatus {
    pub code: StatusCode,
    pub dropped_records: u64,
    pub written_records: u64,
}
impl Default for DiagnosticStatus {
    fn default() -> Self {
        Self {
            code: StatusCode::NotStarted,
            dropped_records: 0,
            written_records: 0,
        }
    }
}
#[derive(Default)]
struct Health {
    failure: AtomicU8,
    accepted: AtomicU64,
    written: AtomicU64,
}
pub struct Diagnostics {
    dispatch: Option<tracing::Dispatch>,
    guard: Mutex<Option<WorkerGuard>>,
    health: Arc<Health>,
    drops: Option<ErrorCounter>,
}
impl Diagnostics {
    pub fn unavailable() -> Self {
        let health = Arc::new(Health::default());
        health.failure.store(1, Ordering::Relaxed);
        Self {
            dispatch: None,
            guard: Mutex::new(None),
            health,
            drops: None,
        }
    }
    /// Dedicated app-local directory only. Build failures never expose paths/errors.
    pub fn open(directory: &Path) -> Self {
        let health = Arc::new(Health::default());
        let writer = match writer::BoundedWriter::open(directory, health.clone()) {
            Ok(writer) => writer,
            Err(_) => return Self::unavailable(),
        };
        Self::with_writer(writer, health)
    }
    fn with_writer(writer: impl std::io::Write + Send + 'static, health: Arc<Health>) -> Self {
        let (output, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
            .buffered_lines_limit(128)
            .lossy(true)
            .thread_name("jentrics-diagnostics")
            .finish(writer);
        let drops = output.error_counter();
        let subscriber = tracing_subscriber::registry().with(layer::FixedLayer {
            output: Mutex::new(output),
            health: health.clone(),
        });
        Self {
            dispatch: Some(tracing::Dispatch::new(subscriber)),
            guard: Mutex::new(Some(guard)),
            health,
            drops: Some(drops),
        }
    }
    pub fn record(&self, code: EventCode, duration_ms: u64, count: u64) {
        let guard = self.guard.lock().expect("diagnostic guard lock poisoned");
        if guard.is_none() {
            return;
        }
        if let Some(dispatch) = &self.dispatch {
            tracing::dispatcher::with_default(dispatch, || {
                tracing::event!(target:"jentrics::diagnostics",tracing::Level::INFO,code=code.as_str(),duration_ms,count);
            });
        }
    }
    pub fn status(&self) -> DiagnosticStatus {
        let dropped_records = self
            .drops
            .as_ref()
            .map_or(0, |counter| counter.dropped_lines() as u64);
        let failure = self.health.failure.load(Ordering::Relaxed);
        let code = match failure {
            1 => StatusCode::InitializationUnavailable,
            2 => StatusCode::WriteUnavailable,
            3 => StatusCode::CapacityReached,
            4 => StatusCode::ShutdownIncomplete,
            _ if dropped_records > 0 => StatusCode::QueueDropped,
            _ if self
                .guard
                .lock()
                .expect("diagnostic guard lock poisoned")
                .is_none() =>
            {
                StatusCode::Stopped
            }
            _ => StatusCode::Healthy,
        };
        DiagnosticStatus {
            code,
            dropped_records,
            written_records: self.health.written.load(Ordering::Relaxed),
        }
    }
    /// The upstream guard bounds shutdown to about 1.1s. Check acknowledged
    /// records afterwards so a timeout is not described as a successful flush.
    pub fn shutdown(&self) {
        let mut guard = self.guard.lock().expect("diagnostic guard lock poisoned");
        if let Some(worker) = guard.take() {
            drop(worker);
            let lost = self
                .drops
                .as_ref()
                .map_or(0, |counter| counter.dropped_lines() as u64);
            if self.health.failure.load(Ordering::Relaxed) == 0
                && self.health.accepted.load(Ordering::Relaxed)
                    > self
                        .health
                        .written
                        .load(Ordering::Relaxed)
                        .saturating_add(lost)
            {
                self.health.failure.store(4, Ordering::Relaxed);
            }
        }
    }
}
impl Drop for Diagnostics {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{self, Write},
        sync::{mpsc, Condvar},
        time::Duration,
    };
    struct Gated {
        writer: writer::BoundedWriter,
        gate: Arc<(Mutex<bool>, Condvar)>,
        started: mpsc::Sender<()>,
    }
    impl Write for Gated {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let _ = self.started.send(());
            let (lock, changed) = &*self.gate;
            let ready = lock.lock().unwrap();
            let _ready = changed
                .wait_timeout_while(ready, Duration::from_secs(2), |ready| !*ready)
                .unwrap();
            self.writer.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.writer.flush()
        }
    }
    #[test]
    fn bounded_queue_reports_drops_and_shutdown_drains_accepted_records() {
        let dir = tempfile::tempdir().unwrap();
        let health = Arc::new(Health::default());
        let writer = writer::BoundedWriter::open(dir.path(), health.clone()).unwrap();
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let (started, rx) = mpsc::channel();
        let diagnostics = Diagnostics::with_writer(
            Gated {
                writer,
                gate: gate.clone(),
                started,
            },
            health,
        );
        diagnostics.record(EventCode::Startup, 0, 0);
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
        for _ in 0..300 {
            diagnostics.record(EventCode::RefreshCompleted, 0, 0);
        }
        assert_eq!(diagnostics.status().code, StatusCode::QueueDropped);
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        diagnostics.shutdown();
        let status = diagnostics.status();
        assert_eq!(status.dropped_records + status.written_records, 301);
    }
}
