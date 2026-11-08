use jentrics_core::{
    metrics::MetricSettings,
    model::{AgentState, DiagnosticCode},
    state::{AppState, Collector},
    storage::Storage,
};
use serde_json::json;
use std::{
    future::Future,
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    path::Path,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

struct NoProvider;
impl Collector for NoProvider {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async { panic!("The synthetic burst must never invoke a provider") })
    }
}

fn post(body: &[u8]) {
    assert!(body.len() <= 65_536);
    let mut stream = TcpStream::connect("127.0.0.1:43190").unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "POST /v1/metrics HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
}

fn open(path: &Path) -> Arc<AppState> {
    AppState::new(Storage::open(path), Arc::new(NoProvider))
}

#[tokio::test]
async fn two_thousand_point_http_burst_persists_once_across_collector_restart() {
    // Uses the real listener/parser/engine/transaction in an isolated database.
    // This is functional burst evidence, separate from installed-app CPU profiling.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("burst.sqlite");
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../fixtures/telemetry-burst.json")).unwrap();
    let end = jentrics_core::state::now_ms() - 1;
    let mut batches = Vec::new();
    for offset in (0..2000).step_by(125) {
        let points: Vec<_> = (offset..offset + 125)
            .map(|index| {
                let start = (end - 2000 + index) * 1_000_000;
                json!({
                    "startTimeUnixNano": start.to_string(),
                    "timeUnixNano": (start + 1_000_000).to_string(),
                    "asInt": fixture["tokensPerPoint"].as_i64().unwrap().to_string(),
                    "attributes": [
                        {"key":"type","value":{"stringValue":fixture["type"]}},
                        {"key":"model","value":{"stringValue":fixture["model"]}}
                    ]
                })
            })
            .collect();
        batches.push(
            serde_json::to_vec(&json!({"resourceMetrics":[{
                "resource":{"attributes":[
                    {"key":"session.id","value":{"stringValue":"synthetic-private-burst-session"}},
                    {"key":"prompt","value":{"stringValue":"synthetic-private-burst-prompt"}}
                ]},
                "scopeMetrics":[{"metrics":[{
                    "name":fixture["metric"],"unit":"tokens",
                    "sum":{"aggregationTemporality":1,"isMonotonic":true,"dataPoints":points}
                }]}]
            }]}))
            .unwrap(),
        );
    }
    assert_eq!(batches.len(), 16);
    let state = open(&path);
    assert!(!state.metrics_view(30).unwrap().running);
    let enabled = state
        .save_metric_settings(MetricSettings {
            enabled: true,
            ..MetricSettings::default()
        })
        .unwrap();
    assert!(enabled.running);
    let started = Instant::now();
    for batch in &batches {
        post(batch);
    }
    let elapsed = started.elapsed();
    let view = state.metrics_view(30).unwrap();
    assert!(view.running && !view.history.truncated);
    assert_eq!(view.status.rejected_batches, 0);
    assert_eq!(view.history.events.len(), 2000);
    assert_eq!(
        view.history.daily.iter().map(|row| row.value).sum::<f64>(),
        2000.0
    );
    state.stop().await;
    drop(state);

    let reopened = open(&path);
    reopened.start_saved_metrics();
    assert!(reopened.metrics_view(30).unwrap().running);
    for batch in &batches {
        post(batch);
    }
    let replayed = reopened.metrics_view(30).unwrap();
    assert_eq!(replayed.status.rejected_batches, 0);
    assert_eq!(replayed.history.events.len(), 2000);
    assert_eq!(
        replayed
            .history
            .daily
            .iter()
            .map(|row| row.value)
            .sum::<f64>(),
        2000.0
    );
    assert!(!serde_json::to_string(&replayed)
        .unwrap()
        .contains("synthetic-private-burst"));
    reopened.stop().await;
    drop(reopened);
    for entry in std::fs::read_dir(directory.path()).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(!bytes
            .windows(b"synthetic-private-burst".len())
            .any(|window| window == b"synthetic-private-burst"));
    }
    println!("Synthetic burst: 2000 points, 16 HTTP requests, {} ms; restart replay preserved exactly 2000 tokens. Not an installed-app resource benchmark.", elapsed.as_millis());
}
