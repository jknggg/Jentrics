use jentrics_core::opencode;
use jentrics_core::process::ProbeOutput;
use opencode::{NativeGate, OpenCodeError};

fn output(text: &str) -> ProbeOutput {
    ProbeOutput {
        exit_code: Some(0),
        stdout: text.as_bytes().to_vec(),
        stderr: Vec::new(),
    }
}

#[test]
fn source_candidate_requires_exact_version_and_advertised_json_but_never_grants_native_execution() {
    assert_eq!(
        opencode::qualify(
            &output("2.2.2\n"),
            Some(&output("stats --json --days --standalone"))
        )
        .unwrap()
        .gate,
        NativeGate::NativeIsolationUnqualified
    );
    assert_eq!(
        opencode::qualify(
            &output("opencode 2.2.2\n"),
            Some(&output("stats --format json"))
        )
        .unwrap()
        .gate,
        NativeGate::StructuredOutputUnavailable
    );
    for version in ["1.19.3", "2.2.3", "9.9.9"] {
        assert_eq!(
            opencode::qualify(&output(version), None).unwrap().gate,
            NativeGate::UnsupportedVersion
        );
    }
    assert!(opencode::qualify(&output("private path output"), None).is_err());
    let mut failed = output("2.2.2");
    failed.exit_code = Some(1);
    assert_eq!(
        opencode::qualify(&failed, None),
        Err(OpenCodeError::VersionUnavailable)
    );
}

#[test]
fn numeric_source_fixture_discards_tool_model_and_future_private_fields() {
    let result = opencode::parse_stats(
        "2.2.2",
        include_bytes!("../../fixtures/opencode-stats-v2-candidate.json"),
        1793318400000,
    )
    .unwrap();
    assert_eq!(result.input_tokens, 1000);
    assert_eq!(result.output_tokens, 200);
    assert_eq!(result.reasoning_tokens, 50);
    assert_eq!(result.cache_read_tokens, 300);
    assert_eq!(result.cache_write_tokens, 100);
    assert_eq!(result.estimated_cost_usd, 0.125);
    assert_eq!(result.range_from_ms, 1790726400000);
    let serialized = serde_json::to_string(&result).unwrap();
    for private in ["private", "model", "tools", "prompts", "providerID"] {
        assert!(!serialized.contains(private));
    }
}

#[test]
fn unsupported_tables_missing_values_and_invalid_ranges_never_become_zero() {
    assert_eq!(
        opencode::parse_stats("1.19.3", b"{}", 1000),
        Err(OpenCodeError::UnsupportedVersion)
    );
    for bytes in [
        b"COST & TOKENS Input 1k".as_slice(),
        b"{}",
        b"null",
        b"{\"tokens\":null}",
    ] {
        assert!(opencode::parse_stats("2.2.2", bytes, 1000).is_err());
    }
    assert_eq!(
        opencode::parse_stats("2.2.2", &vec![b' '; 65537], 1000),
        Err(OpenCodeError::TooLarge)
    );
    let mut payload: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../fixtures/opencode-stats-v2-candidate.json"
    ))
    .unwrap();
    for value in [
        serde_json::json!(-1),
        serde_json::json!(null),
        serde_json::json!(1.5),
    ] {
        payload["tokens"]["input"] = value;
        assert!(
            opencode::parse_stats("2.2.2", &serde_json::to_vec(&payload).unwrap(), 1000).is_err()
        );
    }
}

#[tokio::test]
async fn forged_candidate_cannot_start_a_stats_server_and_missing_cli_is_explicit() {
    use std::path::Path;
    let qualification = opencode::Qualification {
        version: "2.2.2".into(),
        gate: NativeGate::NativeIsolationUnqualified,
    };
    assert_eq!(
        opencode::read_native_stats(
            Path::new("missing.exe"),
            Path::new("missing-directory"),
            &qualification,
            1000
        )
        .await,
        Err(OpenCodeError::NativeIsolationUnqualified)
    );
    assert_eq!(
        opencode::inspect(Path::new("missing.exe"), Path::new("missing-directory")).await,
        Err(OpenCodeError::ProbeFailed)
    );
}
