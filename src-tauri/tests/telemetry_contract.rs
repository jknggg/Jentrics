use jentrics_core::telemetry::{Metric, Provider, TelemetryEngine, TelemetryError};
use serde_json::{json, Value};
static COLLECTOR_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn payload(
    provider: &str,
    metric: &str,
    temporality: u8,
    start: u64,
    end: u64,
    value: f64,
) -> Value {
    json!({"resourceMetrics":[{"resource":{"attributes":[{"key":"session.id","value":{"stringValue":provider}}]},
    "scopeMetrics":[{"scope":{"name":"synthetic"},"metrics":[{"name":metric,"unit":"tokens","sum":{"aggregationTemporality":temporality,"isMonotonic":true,"dataPoints":[{
        "startTimeUnixNano":start.to_string(),"timeUnixNano":end.to_string(),"asDouble":value,
        "attributes":[{"key":"type","value":{"stringValue":"input"}},{"key":"model","value":{"stringValue":"claude-test"}},{"key":"prompt","value":{"stringValue":"private secret"}}]
    }]}}]}]}]})
}

#[test]
fn delta_replay_is_deduplicated_and_private_metadata_never_leaves_engine() {
    let mut engine = TelemetryEngine::default();
    let input = payload(
        "secret-session",
        "claude_code.token.usage",
        1,
        1_000_000,
        2_000_000,
        12.0,
    );
    let result = engine
        .ingest_json(&serde_json::to_vec(&input).unwrap())
        .unwrap();
    assert_eq!(result.deltas.len(), 1);
    assert_eq!(result.deltas[0].provider, Provider::Claude);
    assert_eq!(result.deltas[0].metric, Metric::Tokens);
    assert_eq!(result.deltas[0].value, 12.0);
    assert_eq!(result.deltas[0].dimension.as_deref(), Some("input"));
    assert!(result.coverage_since_collector_start_only);
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains("secret"));
    assert!(!serialized.contains("prompt"));
    let replay = engine
        .ingest_json(&serde_json::to_vec(&input).unwrap())
        .unwrap();
    assert!(replay.deltas.is_empty());
    assert_eq!(replay.duplicate_points, 1);
}

#[test]
fn cumulative_counters_use_baselines_differences_and_explicit_resets() {
    let mut engine = TelemetryEngine::default();
    for (start, end, value, expected, baseline) in [
        (1_000_000, 2_000_000, 100.0, None, 1),
        (1_000_000, 3_000_000, 115.0, Some(15.0), 0),
        (4_000_000, 5_000_000, 4.0, None, 1),
        (4_000_000, 6_000_000, 9.0, Some(5.0), 0),
    ] {
        let result = engine
            .ingest_json(
                &serde_json::to_vec(&payload(
                    "session",
                    "claude_code.token.usage",
                    2,
                    start,
                    end,
                    value,
                ))
                .unwrap(),
            )
            .unwrap();
        assert_eq!(result.baseline_points, baseline);
        assert_eq!(result.deltas.first().map(|delta| delta.value), expected);
    }
    let backwards = payload(
        "session",
        "claude_code.token.usage",
        2,
        4_000_000,
        7_000_000,
        1.0,
    );
    assert_eq!(
        engine.ingest_json(&serde_json::to_vec(&backwards).unwrap()),
        Err(TelemetryError::Conflict)
    );
}

#[test]
fn malformed_unknown_logs_and_missing_identity_fail_without_raw_diagnostics() {
    let mut engine = TelemetryEngine::default();
    for value in [
        json!(null),
        json!({"resourceLogs":[{"body":"private secret"}]}),
        payload("session", "unrecognized", 1, 1, 2, 3.0),
    ] {
        assert!(engine
            .ingest_json(&serde_json::to_vec(&value).unwrap())
            .is_err());
    }
    let mut missing = payload("session", "claude_code.token.usage", 1, 1, 2, 3.0);
    missing["resourceMetrics"][0]["resource"]["attributes"] = json!([]);
    assert_eq!(
        engine.ingest_json(&serde_json::to_vec(&missing).unwrap()),
        Err(TelemetryError::IdentityUnavailable)
    );
    assert_eq!(
        engine.ingest_json(&vec![b' '; 65537]),
        Err(TelemetryError::TooLarge)
    );
}

#[test]
fn series_are_separate_order_independent_and_rejected_batches_are_atomic() {
    let mut engine = TelemetryEngine::default();
    let a = payload(
        "one",
        "claude_code.token.usage",
        1,
        1_000_000,
        2_000_000,
        10.0,
    );
    engine
        .ingest_json(&serde_json::to_vec(&a).unwrap())
        .unwrap();
    let mut reordered = a.clone();
    reordered["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]
        ["attributes"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(
        engine
            .ingest_json(&serde_json::to_vec(&reordered).unwrap())
            .unwrap()
            .duplicate_points,
        1
    );
    let separate = payload(
        "two",
        "claude_code.token.usage",
        1,
        1_000_000,
        2_000_000,
        10.0,
    );
    assert_eq!(
        engine
            .ingest_json(&serde_json::to_vec(&separate).unwrap())
            .unwrap()
            .deltas[0]
            .value,
        10.0
    );
    let overlap = payload(
        "one",
        "claude_code.token.usage",
        1,
        1_500_000,
        3_000_000,
        20.0,
    );
    assert_eq!(
        engine.ingest_json(&serde_json::to_vec(&overlap).unwrap()),
        Err(TelemetryError::Conflict)
    );
    let next = payload(
        "one",
        "claude_code.token.usage",
        1,
        2_000_000,
        3_000_000,
        20.0,
    );
    let mut invalid_batch = next.clone();
    invalid_batch["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"]
        .as_array_mut()
        .unwrap()
        .push(
            overlap["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]
                .clone(),
        );
    assert!(engine
        .ingest_json(&serde_json::to_vec(&invalid_batch).unwrap())
        .is_err());
    assert_eq!(
        engine
            .ingest_json(&serde_json::to_vec(&next).unwrap())
            .unwrap()
            .deltas[0]
            .value,
        20.0
    );
}

#[test]
fn gemini_tokens_and_histogram_totals_have_distinct_units_and_coverage() {
    let mut engine = TelemetryEngine::default();
    let tokens = payload(
        "gemini-session",
        "gemini_cli.token.usage",
        1,
        1_000_000,
        2_000_000,
        8.0,
    );
    let result = engine
        .ingest_json(&serde_json::to_vec(&tokens).unwrap())
        .unwrap();
    assert_eq!(result.deltas[0].provider, Provider::Gemini);
    let histogram = json!({"resourceMetrics":[{"resource":{"attributes":[{"key":"session.id","value":{"stringValue":"gemini-session"}}]},"scopeMetrics":[{"metrics":[{"name":"gemini_cli.api.request.latency","unit":"ms","histogram":{"aggregationTemporality":1,"dataPoints":[{"startTimeUnixNano":"1000000","timeUnixNano":"2000000","sum":120.0,"count":"2","attributes":[{"key":"model","value":{"stringValue":"gemini-test"}}]}]}}]}]}]});
    let result = engine
        .ingest_json(&serde_json::to_vec(&histogram).unwrap())
        .unwrap();
    assert_eq!(result.deltas[0].metric, Metric::RequestLatencyMs);
    assert_eq!(result.deltas[0].value, 120.0);
    assert_eq!(result.deltas[0].count, Some(2));
    // Count/sum supports a mean, never an invented percentile distribution.
    assert_eq!(
        result.deltas[0].value / result.deltas[0].count.unwrap() as f64,
        60.0
    );
    let mut cumulative = histogram.clone();
    cumulative["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["histogram"]
        ["aggregationTemporality"] = json!(2);
    let mut baseline = TelemetryEngine::default();
    baseline
        .ingest_json(&serde_json::to_vec(&cumulative).unwrap())
        .unwrap();
    cumulative["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["histogram"]["dataPoints"]
        [0]["timeUnixNano"] = json!("3000000");
    cumulative["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["histogram"]["dataPoints"]
        [0]["sum"] = json!(130);
    assert!(
        baseline
            .ingest_json(&serde_json::to_vec(&cumulative).unwrap())
            .is_err(),
        "histogram sum cannot increase without an observation"
    );
}

#[test]
fn invalid_model_types_and_counter_bounds_cannot_be_silently_accepted() {
    let mut engine = TelemetryEngine::default();
    let mut input = payload("session", "claude_code.token.usage", 1, 1, 2, 3.0);
    input["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]
        ["attributes"][1]["value"] = json!({"intValue":"55"});
    assert_eq!(
        engine.ingest_json(&serde_json::to_vec(&input).unwrap()),
        Err(TelemetryError::Malformed)
    );
    for value in [-1.0, 1.5, 9_007_199_254_740_992.0] {
        let input = payload("session", "claude_code.token.usage", 1, 1, 2, value);
        assert!(engine
            .ingest_json(&serde_json::to_vec(&input).unwrap())
            .is_err());
    }
    let mut invalid_dimension = payload("session", "claude_code.token.usage", 1, 1, 2, 3.0);
    invalid_dimension["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"]
        [0]["attributes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"query_source","value":{"stringValue":"private arbitrary text"}}));
    assert_eq!(
        engine.ingest_json(&serde_json::to_vec(&invalid_dimension).unwrap()),
        Err(TelemetryError::UnsupportedShape)
    );
}

#[test]
fn capacity_and_out_of_order_replays_are_explicit_without_erasing_known_state() {
    let mut engine = TelemetryEngine::default();
    for index in 0..1024 {
        let input = payload(
            &format!("session-{index}"),
            "claude_code.token.usage",
            1,
            1_000_000,
            2_000_000,
            1.0,
        );
        engine
            .ingest_json(&serde_json::to_vec(&input).unwrap())
            .unwrap();
    }
    let extra = payload(
        "one-too-many",
        "claude_code.token.usage",
        1,
        1_000_000,
        2_000_000,
        1.0,
    );
    assert_eq!(
        engine.ingest_json(&serde_json::to_vec(&extra).unwrap()),
        Err(TelemetryError::Capacity)
    );
    let next = payload(
        "session-0",
        "claude_code.token.usage",
        1,
        2_000_000,
        3_000_000,
        3.0,
    );
    assert_eq!(
        engine
            .ingest_json(&serde_json::to_vec(&next).unwrap())
            .unwrap()
            .deltas[0]
            .value,
        3.0
    );
    let replay = payload(
        "session-0",
        "claude_code.token.usage",
        1,
        1_000_000,
        2_000_000,
        1.0,
    );
    assert_eq!(
        engine.ingest_json(&serde_json::to_vec(&replay).unwrap()),
        Err(TelemetryError::OutOfOrder)
    );
    // A fresh engine cannot claim a persisted dedup baseline from the old runtime.
    let mut restarted = TelemetryEngine::default();
    let cumulative = payload(
        "session-0",
        "claude_code.token.usage",
        2,
        1_000_000,
        4_000_000,
        500.0,
    );
    let result = restarted
        .ingest_json(&serde_json::to_vec(&cumulative).unwrap())
        .unwrap();
    assert_eq!(result.baseline_points, 1);
    assert!(result.deltas.is_empty());
    assert!(result.coverage_since_collector_start_only);
}

#[test]
fn collector_stays_disabled_and_loopback_http_is_bounded_explicit_and_stoppable() {
    let _guard = COLLECTOR_TEST.lock().unwrap();
    use jentrics_core::telemetry::server::{Collector, ENDPOINT};
    use std::{
        io::{Read, Write},
        net::{Shutdown, TcpStream},
        time::{Duration, Instant},
    };
    assert!(matches!(
        Collector::start(false),
        Err(TelemetryError::Disabled)
    ));
    assert_eq!(ENDPOINT, "http://127.0.0.1:43190/v1/metrics");
    let mut collector = Collector::start(true).unwrap();
    let request = |headers: &str, body: &[u8]| {
        let mut stream = TcpStream::connect("127.0.0.1:43190").unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream.write_all(headers.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    };
    let body = serde_json::to_vec(&payload(
        "http-session",
        "claude_code.token.usage",
        1,
        1_000_000,
        2_000_000,
        7.0,
    ))
    .unwrap();
    let response=request(&format!("POST /v1/metrics HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",body.len()),&body);
    assert!(response.starts_with("HTTP/1.1 200"));
    assert_eq!(collector.try_recv().unwrap().unwrap().deltas[0].value, 7.0);
    for headers in [
        "POST /v1/metrics HTTP/1.1\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n",
        "POST /v1/logs HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n",
        "POST /v1/metrics HTTP/1.1\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n",
        "POST /v1/metrics HTTP/1.1\r\nContent-Type: application/json\r\nContent-Encoding: gzip\r\nContent-Length: 0\r\n\r\n",
    ] {
        assert!(request(headers,b"").starts_with("HTTP/1.1 415"));
        assert!(collector.try_recv().unwrap().is_err());
    }
    let oversized="POST /v1/metrics HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: 65537\r\n\r\n";
    assert!(request(oversized, b"").starts_with("HTTP/1.1 413"));
    assert_eq!(collector.try_recv().unwrap(), Err(TelemetryError::TooLarge));
    for index in 0..33 {
        let body = serde_json::to_vec(&payload(
            &format!("queue-session-{index}"),
            "claude_code.token.usage",
            1,
            1_000_000,
            2_000_000,
            7.0,
        ))
        .unwrap();
        let response=request(&format!("POST /v1/metrics HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",body.len()),&body);
        assert!(response.starts_with(if index < 32 {
            "HTTP/1.1 200"
        } else {
            "HTTP/1.1 503"
        }));
    }
    for _ in 0..32 {
        collector.try_recv().unwrap().unwrap();
    }
    let body = serde_json::to_vec(&payload(
        "queue-session-32",
        "claude_code.token.usage",
        1,
        1_000_000,
        2_000_000,
        7.0,
    ))
    .unwrap();
    assert!(request(&format!("POST /v1/metrics HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",body.len()),&body).starts_with("HTTP/1.1 200"));
    assert_eq!(collector.try_recv().unwrap().unwrap().deltas[0].value, 7.0);
    let mut partial = TcpStream::connect("127.0.0.1:43190").unwrap();
    partial
        .write_all(b"POST /v1/metrics HTTP/1.1\r\nContent-Length:")
        .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    collector.stop().unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(TcpStream::connect("127.0.0.1:43190").is_err());
}

#[test]
fn sanitized_delta_validation_rejects_corrupt_storage_and_keys_are_stable() {
    use jentrics_core::telemetry::validate_delta;
    let input = serde_json::to_vec(&payload(
        "session",
        "claude_code.token.usage",
        1,
        1_000_001,
        2_000_002,
        3.0,
    ))
    .unwrap();
    let mut first = TelemetryEngine::default();
    let delta = first.ingest_json(&input).unwrap().deltas.remove(0);
    validate_delta(&delta).unwrap();
    let mut restarted = TelemetryEngine::default();
    assert_eq!(
        restarted.ingest_json(&input).unwrap().deltas[0].series_key,
        delta.series_key
    );
    assert_eq!(delta.start_ns, "1000001");
    assert_eq!(delta.end_ns, "2000002");
    assert!(!delta.series_key.contains("session"));
    let mut invalid = delta.clone();
    invalid.start_ms = 55;
    assert!(validate_delta(&invalid).is_err());
    invalid = delta.clone();
    invalid.value = -1.0;
    assert!(validate_delta(&invalid).is_err());
    invalid = delta.clone();
    invalid.dimension = Some("private".into());
    assert!(validate_delta(&invalid).is_err());
    invalid = delta;
    invalid.series_key = "raw session".into();
    assert!(validate_delta(&invalid).is_err());
}

#[test]
fn attributed_series_are_rejected_and_unknown_private_attributes_do_not_enter_fingerprint() {
    let input = payload(
        "session",
        "claude_code.token.usage",
        1,
        1_000_000,
        2_000_000,
        3.0,
    );
    let mut engine = TelemetryEngine::default();
    let first = engine
        .ingest_json(&serde_json::to_vec(&input).unwrap())
        .unwrap();
    let mut changed = input.clone();
    changed["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]
        ["attributes"][2]["value"] = json!({"stringValue":"different private text"});
    assert_eq!(
        engine
            .ingest_json(&serde_json::to_vec(&changed).unwrap())
            .unwrap()
            .duplicate_points,
        1
    );
    assert!(!first.deltas[0].series_key.contains("private"));
    for name in [
        "agent.name",
        "skill.name",
        "plugin.name",
        "marketplace.name",
        "mcp_server.name",
        "mcp_tool.name",
    ] {
        let mut changed = input.clone();
        changed["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]
            ["attributes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"key":name,"value":{"stringValue":"private name"}}));
        assert_eq!(
            engine.ingest_json(&serde_json::to_vec(&changed).unwrap()),
            Err(TelemetryError::UnsupportedShape)
        );
    }
}

#[test]
fn sink_persistence_failure_returns_503_and_allows_a_retry_without_losing_delta() {
    use jentrics_core::telemetry::server::Collector;
    use std::{
        io::{Read, Write},
        net::{Shutdown, TcpStream},
        sync::atomic::{AtomicBool, Ordering},
        sync::{Arc, Mutex},
        time::Duration,
    };
    let _guard = COLLECTOR_TEST.lock().unwrap();
    let fail = Arc::new(AtomicBool::new(true));
    let received = Arc::new(Mutex::new(Vec::new()));
    let sink_fail = fail.clone();
    let sink_received = received.clone();
    let mut collector = Collector::start_with_sink(
        true,
        Arc::new(move |result| {
            if sink_fail.load(Ordering::SeqCst) {
                return Err(TelemetryError::Io);
            }
            sink_received.lock().unwrap().push(result?);
            Ok(())
        }),
    )
    .unwrap();
    let body = serde_json::to_vec(&payload(
        "sink-session",
        "claude_code.token.usage",
        1,
        1_000_000,
        2_000_000,
        9.0,
    ))
    .unwrap();
    let send = || {
        let mut stream = TcpStream::connect("127.0.0.1:43190").unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(stream,"POST /v1/metrics HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",body.len()).unwrap();
        stream.write_all(&body).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    };
    assert!(send().starts_with("HTTP/1.1 503"));
    fail.store(false, Ordering::SeqCst);
    assert!(send().starts_with("HTTP/1.1 200"));
    assert_eq!(received.lock().unwrap()[0].deltas[0].value, 9.0);
    assert!(send().starts_with("HTTP/1.1 200"));
    assert!(received.lock().unwrap()[1].deltas.is_empty());
    collector.stop().unwrap();
}
