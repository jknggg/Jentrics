// Explicit opt-in loopback-only OTLP/HTTP JSON receiver; no default listener.
use super::{IngestResult, TelemetryEngine, TelemetryError};
use std::{
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
mod http;

pub const ENDPOINT: &str = "http://127.0.0.1:43190/v1/metrics";
pub type Sink =
    Arc<dyn Fn(Result<IngestResult, TelemetryError>) -> Result<(), TelemetryError> + Send + Sync>;
const ADDRESS: &str = "127.0.0.1:43190";

pub struct Collector {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), TelemetryError>>>,
    receiver: Option<Receiver<Result<IngestResult, TelemetryError>>>,
}

enum Delivery {
    Queue(SyncSender<Result<IngestResult, TelemetryError>>),
    Sink(Sink),
}

impl Delivery {
    fn deliver(&self, result: Result<IngestResult, TelemetryError>) -> Result<(), TelemetryError> {
        match self {
            Self::Sink(sink) => sink(result),
            Self::Queue(sender) => sender.try_send(result).map_err(|error| match error {
                TrySendError::Full(_) => TelemetryError::QueueFull,
                TrySendError::Disconnected(_) => TelemetryError::WorkerFailed,
            }),
        }
    }
}

impl Collector {
    pub fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }
    /// Sink runs on the collector worker. It must finish promptly and must not
    /// wait on the mutex holding this Collector. A failed sink returns HTTP503;
    /// no in-memory interval state commits until the sink accepts the batch.
    pub fn start_with_sink(enabled: bool, sink: Sink) -> Result<Self, TelemetryError> {
        Self::start_delivery(enabled, Delivery::Sink(sink), None)
    }
    pub fn start(enabled: bool) -> Result<Self, TelemetryError> {
        let (sender, receiver) = mpsc::sync_channel(32);
        Self::start_delivery(enabled, Delivery::Queue(sender), Some(receiver))
    }

    fn start_delivery(
        enabled: bool,
        delivery: Delivery,
        receiver: Option<Receiver<Result<IngestResult, TelemetryError>>>,
    ) -> Result<Self, TelemetryError> {
        if !enabled {
            return Err(TelemetryError::Disabled);
        }
        let listener = TcpListener::bind(ADDRESS).map_err(|_| TelemetryError::Io)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| TelemetryError::Io)?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("jentrics-local-metrics".into())
            .spawn(move || run(listener, worker_stop, delivery))
            .map_err(|_| TelemetryError::Io)?;
        Ok(Self {
            stop,
            worker: Some(worker),
            receiver,
        })
    }

    pub fn try_recv(&self) -> Result<Result<IngestResult, TelemetryError>, TryRecvError> {
        self.receiver
            .as_ref()
            .ok_or(TryRecvError::Disconnected)?
            .try_recv()
    }

    pub fn stop(&mut self) -> Result<(), TelemetryError> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            return worker.join().map_err(|_| TelemetryError::WorkerFailed)?;
        }
        Ok(())
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("Local telemetry collector shutdown failed: {error:?}");
        }
    }
}

fn run(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    delivery: Delivery,
) -> Result<(), TelemetryError> {
    let mut engine = TelemetryEngine::default();
    while !stop.load(Ordering::Acquire) {
        let (mut stream, peer) = match listener.accept() {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
                continue;
            }
            Err(_) => return Err(TelemetryError::Io),
        };
        if !peer.ip().is_loopback() {
            return Err(TelemetryError::Io);
        }
        let mut candidate = engine.clone();
        let result =
            http::read_request(&mut stream, &stop).and_then(|body| candidate.ingest_json(&body));
        let status = match result {
            Ok(result) => match delivery.deliver(Ok(result)) {
                Ok(()) => {
                    engine = candidate;
                    200
                }
                Err(TelemetryError::WorkerFailed) => return Err(TelemetryError::WorkerFailed),
                Err(_) => 503,
            },
            Err(error) => {
                if let Err(delivery_error) = delivery.deliver(Err(error)) {
                    if delivery_error == TelemetryError::WorkerFailed {
                        return Err(delivery_error);
                    }
                    http::respond(&mut stream, 503)?;
                    continue;
                }
                match error {
                    TelemetryError::TooLarge => 413,
                    TelemetryError::UnsupportedShape | TelemetryError::UnsupportedSignal => 415,
                    TelemetryError::Conflict | TelemetryError::OutOfOrder => 409,
                    _ => 400,
                }
            }
        };
        // The peer can disconnect after a valid export; a response failure is
        // surfaced as numeric diagnostics and never rolls back an accepted batch.
        if http::respond(&mut stream, status).is_err() {
            delivery.deliver(Err(TelemetryError::Io))?;
        }
    }
    Ok(())
}
