use jentrics_core::{
    model::{AgentState, DiagnosticCode},
    state::{AppState, Collector, MonitorError, MonitorSettings},
    storage::{PreferenceKey, Storage, StorageError},
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::mpsc;
struct CountCollector {
    calls: AtomicUsize,
    started: mpsc::UnboundedSender<usize>,
}
impl Collector for CountCollector {
    fn collect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentState>, DiagnosticCode>> + Send + '_>> {
        Box::pin(async {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            self.started.send(call).unwrap();
            Ok(vec![])
        })
    }
}
fn setup(
    storage: Result<Storage, StorageError>,
) -> (
    Arc<AppState>,
    Arc<CountCollector>,
    mpsc::UnboundedReceiver<usize>,
) {
    let (started, receiver) = mpsc::unbounded_channel();
    let collector = Arc::new(CountCollector {
        calls: AtomicUsize::new(0),
        started,
    });
    (
        AppState::new(storage, collector.clone()),
        collector,
        receiver,
    )
}
async fn check(receiver: &mut mpsc::UnboundedReceiver<usize>) -> usize {
    tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap()
}
async fn no_check(receiver: &mut mpsc::UnboundedReceiver<usize>) {
    assert!(
        tokio::time::timeout(Duration::from_millis(40), receiver.recv())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn absent_preferences_default_to_enabled_five_minutes_and_persist_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let (state, _, _) = setup(Storage::open(&path));
    let view = state.monitor_settings();
    assert_eq!(
        view.settings,
        Some(MonitorSettings {
            paused: false,
            interval_minutes: 5
        })
    );
    assert!(view.error.is_none());
    state
        .save_monitor_settings(MonitorSettings {
            paused: true,
            interval_minutes: 15,
        })
        .unwrap();
    let (reopened, _, _) = setup(Storage::open(&path));
    assert_eq!(
        reopened.monitor_settings().settings,
        Some(MonitorSettings {
            paused: true,
            interval_minutes: 15
        })
    );
    assert_eq!(
        state.save_monitor_settings(MonitorSettings {
            paused: false,
            interval_minutes: 1
        }),
        Err(MonitorError::InvalidSettings)
    );
}
#[tokio::test]
async fn production_pause_resume_and_manual_refresh_do_not_wait_for_old_timer() {
    let directory = tempfile::tempdir().unwrap();
    let (state, _, mut started) = setup(Storage::open(&directory.path().join("state.sqlite")));
    state
        .save_monitor_settings(MonitorSettings {
            paused: true,
            interval_minutes: 30,
        })
        .unwrap();
    let task = tokio::spawn(state.clone().run_monitor());
    no_check(&mut started).await;
    state.refresh().await;
    assert_eq!(check(&mut started).await, 1);
    state
        .save_monitor_settings(MonitorSettings {
            paused: false,
            interval_minutes: 30,
        })
        .unwrap();
    assert_eq!(check(&mut started).await, 2);
    state
        .save_monitor_settings(MonitorSettings {
            paused: true,
            interval_minutes: 30,
        })
        .unwrap();
    no_check(&mut started).await;
    state
        .save_monitor_settings(MonitorSettings {
            paused: false,
            interval_minutes: 5,
        })
        .unwrap();
    assert_eq!(check(&mut started).await, 3);
    state.stop().await;
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    no_check(&mut started).await;
}
#[tokio::test]
async fn unreadable_settings_pause_automatic_checks_but_manual_read_still_works() {
    let (state, _, mut started) = setup(Err(StorageError::Unavailable));
    let view = state.monitor_settings();
    assert_eq!(view.settings, None);
    assert_eq!(view.error, Some(MonitorError::SettingsUnavailable));
    let task = tokio::spawn(state.clone().run_monitor());
    no_check(&mut started).await;
    state.refresh().await;
    assert_eq!(check(&mut started).await, 1);
    assert!(state
        .save_monitor_settings(MonitorSettings {
            paused: false,
            interval_minutes: 5
        })
        .is_err());
    state.stop().await;
    task.await.unwrap();
}
#[tokio::test]
async fn corrupt_preferences_are_not_defaulted_and_explicit_save_recovers() {
    let directory = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(&directory.path().join("state.sqlite")).unwrap();
    storage
        .write_preference(
            PreferenceKey::Monitor,
            &serde_json::json!({"paused":false,"intervalMinutes":"broken"}),
        )
        .unwrap();
    let (state, _, mut started) = setup(Ok(storage));
    assert!(state.monitor_settings().settings.is_none());
    let task = tokio::spawn(state.clone().run_monitor());
    no_check(&mut started).await;
    state
        .save_monitor_settings(MonitorSettings {
            paused: false,
            interval_minutes: 10,
        })
        .unwrap();
    assert_eq!(check(&mut started).await, 1);
    state.stop().await;
    task.await.unwrap();
}
