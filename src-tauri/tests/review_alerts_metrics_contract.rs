use jentrics_core::{
    alerts::{evaluate_alerts, AlertKind, AlertSettings},
    model::*,
    storage::Storage,
    telemetry::{Metric, Provider, TelemetryDelta},
};
const NOW: i64 = 1_800_000_000_000;
fn quota_failure() -> AgentState {
    AgentState {
        agent_id: AgentId::Claude,
        version: Some("2.1.0".into()),
        availability: Availability::Detected,
        auth_state: AuthState::Ready,
        capabilities: Capabilities {
            discovery: true,
            auth_readiness: true,
            quotas: true,
            usage: false,
            session_start: false,
        },
        observed_ms: NOW - 1_800_000,
        diagnostic_code: None,
        account_usage: None,
        quota: Some(QuotaSnapshot {
            source: QuotaSource::ClaudeStatusLine,
            observed_ms: NOW,
            windows: vec![],
            diagnostic_code: Some(DiagnosticCode::QuotaUnavailable),
        }),
    }
}
fn only_errors() -> AlertSettings {
    AlertSettings {
        enabled: true,
        threshold80: false,
        threshold95: false,
        near_reset: false,
        expected_reset: false,
        restored: false,
        errors: true,
        quiet_hours: None,
    }
}
#[test]
fn fresh_capture_error_uses_capture_time_when_readiness_is_old() {
    let events = evaluate_alerts(&only_errors(), &[quota_failure()], &[], &[], NOW, 600).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].key.kind, AlertKind::QuotaError);
    assert_eq!(events[0].key.epoch_ms, Some(NOW));
}
#[test]
fn provider_probe_timeout_is_a_monitoring_error_even_without_a_quota_snapshot() {
    let mut failed = quota_failure();
    failed.agent_id = AgentId::Codex;
    failed.version = Some("0.165.0-alpha.1.2".into());
    failed.observed_ms = NOW;
    failed.quota = None;
    failed.capabilities.quotas = false;
    failed.auth_state = AuthState::Unknown;
    failed.availability = Availability::ProbeFailed;
    failed.diagnostic_code = Some(DiagnosticCode::ProbeTimeout);
    assert_eq!(
        evaluate_alerts(&only_errors(), &[failed], &[], &[], NOW, 600)
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn retained_daily_bucket_keeps_observations_inside_the_configured_period() {
    const DAY: i64 = 86_400_000;
    let dir = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&dir.path().join("review.sqlite")).unwrap();
    let end = DAY + 7_200_000;
    let start = end - 1000;
    let event = TelemetryDelta {
        provider: Provider::Claude,
        metric: Metric::Tokens,
        dimension: Some("input".into()),
        model: Some("claude-test".into()),
        start_ms: start,
        end_ms: end,
        value: 10.0,
        count: None,
        gap_before: false,
        series_key: "defaultHasherV1:0123456789abcdef".into(),
        start_ns: (start * 1_000_000).to_string(),
        end_ns: (end * 1_000_000).to_string(),
        counter_start_ns: (start * 1_000_000).to_string(),
    };
    let now = 31 * DAY + 3_600_000;
    assert_eq!(storage.record_metrics(&[event], now, 30, 30).unwrap(), 1);
    let history = storage.metric_history(0, now).unwrap();
    assert_eq!(history.events.len(), 1);
    assert_eq!(
        history.daily.len(),
        1,
        "The accepted event is only29days23hours old"
    );
}
struct FailedProbe;
impl jentrics_core::state::Collector for FailedProbe {
    fn collect(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>,
    > {
        Box::pin(async {
            let mut failed = quota_failure();
            failed.observed_ms = jentrics_core::state::now_ms();
            failed.availability = Availability::ProbeFailed;
            failed.diagnostic_code = Some(DiagnosticCode::ProbeTimeout);
            failed.quota = None;
            failed.capabilities.quotas = false;
            Ok(vec![failed])
        })
    }
}
#[tokio::test]
async fn failed_provider_probe_receives_the_first_monitor_backoff() {
    use std::{sync::Arc, time::Duration};
    let dir = tempfile::tempdir().unwrap();
    let state = jentrics_core::state::AppState::new(
        Storage::open(&dir.path().join("review.sqlite")),
        Arc::new(FailedProbe),
    );
    let task = tokio::spawn(state.clone().run_monitor());
    let interval = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(interval) = state.monitor_settings().effective_interval_minutes {
                break interval;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    state.stop().await;
    task.await.unwrap();
    assert_eq!(interval, 10);
}

#[cfg(windows)]
#[tokio::test]
async fn telemetry_protocol_diagnostics_return_4xx_and_response_disconnect_keeps_listener_alive() {
    use jentrics_core::{metrics::MetricSettings, state::AppState};
    use std::{
        io::{Read, Write},
        net::{Shutdown, TcpStream},
        os::windows::io::AsRawSocket,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc, Mutex,
        },
        time::Duration,
    };
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(
        Storage::open(&dir.path().join("review.sqlite")),
        Arc::new(FailedProbe),
    );
    state
        .save_metric_settings(MetricSettings {
            enabled: true,
            event_retention_days: 30,
            daily_retention_days: 365,
        })
        .unwrap();
    let request = |body: &str| {
        let mut stream = TcpStream::connect("127.0.0.1:43190").unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write!(stream,"POST /v1/metrics HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",body.len(),body).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    };
    let malformed = request("{");
    let before = state.metrics_view(30).unwrap();
    assert!(before.running);
    assert_eq!(before.status.rejected_batches, 1);
    assert!(malformed.starts_with("HTTP/1.1 400"), "{malformed}");
    // Hold the worker after it records a protocol error but before its HTTP
    // response, then reset the peer. This deterministically exercises write Io.
    let (entered_tx, entered_rx) = mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = mpsc::sync_channel(0);
    let resume_rx = Mutex::new(resume_rx);
    let first = AtomicBool::new(true);
    state.set_publisher(Arc::new(move |_| {
        if first.swap(false, Ordering::SeqCst) {
            entered_tx.send(()).unwrap();
            resume_rx.lock().unwrap().recv().unwrap();
        }
    }));
    let mut stream = TcpStream::connect("127.0.0.1:43190").unwrap();
    stream.write_all(b"POST /v1/metrics HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: 1\r\n\r\n{").unwrap();
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    #[repr(C)]
    struct Linger {
        on: u16,
        seconds: u16,
    }
    #[link(name = "ws2_32")]
    unsafe extern "system" {
        fn setsockopt(socket: usize, level: i32, name: i32, value: *const u8, length: i32) -> i32;
    }
    // Winsock SO_LINGER(1,0) aborts with RST; constants/ABI match windows-sys
    // 0.61.2 WinSock bindings, without adding a production feature/dependency.
    let linger = Linger { on: 1, seconds: 0 };
    assert_eq!(
        unsafe {
            setsockopt(
                stream.as_raw_socket() as usize,
                65535,
                128,
                (&linger as *const Linger).cast(),
                std::mem::size_of::<Linger>() as i32,
            )
        },
        0
    );
    drop(stream);
    resume_tx.send(()).unwrap();
    let after = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let view = state.metrics_view(30).unwrap();
            if view.status.rejected_batches >= 3 || !view.running {
                break view;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let reconnect = request("{");
    let end = jentrics_core::state::now_ms() - 1;
    let body = serde_json::json!({"resourceMetrics":[{"resource":{"attributes":[{"key":"session.id","value":{"stringValue":"SYNTHETIC_PRIVATE_SESSION"}}]},"scopeMetrics":[{"metrics":[{"name":"claude_code.token.usage","unit":"tokens","sum":{"aggregationTemporality":1,"isMonotonic":true,"dataPoints":[{"attributes":[{"key":"type","value":{"stringValue":"input"}},{"key":"model","value":{"stringValue":"claude-test"}},{"key":"prompt","value":{"stringValue":"SYNTHETIC_PRIVATE_PROMPT"}}],"startTimeUnixNano":((end-1)*1_000_000).to_string(),"timeUnixNano":(end*1_000_000).to_string(),"asInt":"5"}]}}]}]}]}).to_string();
    let successful = request(&body);
    let public = state.metrics_view(30).unwrap();
    let public_json = serde_json::to_string(&public).unwrap();
    state.stop().await;
    assert!(successful.starts_with("HTTP/1.1 200"), "{successful}");
    assert_eq!(public.history.events.len(), 1);
    assert_eq!(public.history.daily.len(), 1);
    for private in [
        "SYNTHETIC_PRIVATE",
        "seriesKey",
        "counterStartNs",
        "startNs",
        "endNs",
    ] {
        assert!(
            !public_json.contains(private),
            "Public metrics leaked {private}"
        );
    }
    assert!(after.running);
    assert!(
        after.status.rejected_batches >= 3,
        "response Io diagnostic was not observed"
    );
    assert!(reconnect.starts_with("HTTP/1.1 400"), "{reconnect}");
}
