use jentrics_core::{metrics::MetricSettings, state::AppState, storage::Storage};
use std::{future::Future, pin::Pin, sync::Arc};
struct Empty;
impl jentrics_core::state::Collector for Empty {
    fn collect(
        &self,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Vec<jentrics_core::model::AgentState>,
                        jentrics_core::model::DiagnosticCode,
                    >,
                > + Send
                + '_,
        >,
    > {
        Box::pin(async { Ok(vec![]) })
    }
}
#[tokio::test]
async fn telemetry_defaults_off_and_view_never_leaks_series_identifiers() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let state = AppState::new(
        Storage::open(&dir.path().join("state.sqlite")),
        Arc::new(Empty),
    );
    state.start_saved_metrics();
    let view = state.metrics_view(30).unwrap();
    assert!(!view.settings.enabled && !view.running);
    assert!(state.clear_metric_history(false).is_err());
    let view = state
        .save_metric_settings(MetricSettings::default())
        .unwrap();
    assert!(!view.running);
    let json = serde_json::to_string(&view).unwrap();
    assert!(!json.contains("seriesKey"));
    state.stop().await;
}
