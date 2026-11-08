use crate::{
    state::{AppState, DiscoveryCollector},
    storage::Storage,
};
use std::sync::Arc;
use tauri::{Emitter, Manager};

pub fn build_app() -> Result<tauri::App, tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.len() == 2 && args[1] == "--quit" {
                crate::tray::request_quit(app);
            } else {
                crate::dashboard::request_open_dashboard(app);
            }
        }))
        .invoke_handler(tauri::generate_handler![
            crate::commands::get_dashboard_state,
            crate::settings_commands::export_configuration,
            crate::settings_commands::import_configuration,
            crate::settings_commands::get_display_settings,
            crate::settings_commands::set_display_settings,
            crate::settings_commands::get_storage_maintenance,
            crate::settings_commands::get_adapter_catalog,
            crate::settings_commands::get_diagnostics_status,
            crate::settings_commands::take_settings_request,
            crate::commands::get_history,
            crate::commands::get_history_settings,
            crate::commands::set_history_retention,
            crate::commands::clear_history,
            crate::commands::get_claude_bridge_status,
            crate::commands::prepare_claude_bridge,
            crate::commands::apply_claude_bridge,
            crate::commands::restore_claude_bridge,
            crate::notifications::get_alert_settings,
            crate::notifications::set_alert_settings,
            crate::notifications::request_alert_permission,
            crate::notifications::send_test_notification,
            crate::commands::get_metrics,
            crate::commands::set_metric_settings,
            crate::commands::clear_metric_history,
            crate::commands::get_pricing,
            crate::commands::set_price_override,
            crate::commands::remove_price_override,
            crate::commands::estimate_cost,
            crate::commands::get_monitor_settings,
            crate::commands::set_monitor_settings,
            crate::commands::refresh_agents_command,
            crate::commands::prepare_wake,
            crate::commands::confirm_wake
        ])
        .setup(|app| {
            let args: Vec<_> = std::env::args().collect();
            if args.len() == 2 && args[1] == "--quit" {
                app.handle().exit(0);
                return Ok(());
            }
            let search_dirs = crate::environment::search_dirs()
                .inspect_err(|error| eprintln!("Jentrics startup environment: {error}"))?;
            let collector = Arc::new(DiscoveryCollector {
                search_dirs: search_dirs.clone(),
            });
            let storage = app
                .path()
                .app_local_data_dir()
                .map_err(|_| crate::storage::StorageError::Unavailable)
                .and_then(|dir| {
                    std::fs::create_dir_all(&dir)
                        .map_err(|_| crate::storage::StorageError::Unavailable)?;
                    Storage::open(&dir.join("state.sqlite"))
                });
            let state = AppState::new(storage, collector);
            let diagnostics = match app.path().app_local_data_dir() {
                Ok(directory) => {
                    crate::diagnostics::Diagnostics::open(&directory.join("diagnostics"))
                }
                Err(_) => crate::diagnostics::Diagnostics::unavailable(),
            };
            state.set_diagnostics(Arc::new(diagnostics));
            crate::notifications::install(app.handle(), &state);
            state.start_saved_metrics();
            let bridge_paths = app.path().app_local_data_dir().ok().and_then(|data| {
                let config = std::env::var_os("CLAUDE_CONFIG_DIR")
                    .map(std::path::PathBuf::from)
                    .or_else(|| {
                        std::env::var_os("USERPROFILE")
                            .map(|home| std::path::PathBuf::from(home).join(".claude"))
                    })?;
                if !config.is_absolute() {
                    return None;
                }
                Some((config.join("settings.json"), data.join("claude-capture")))
            });
            if let Some((settings, capture)) = bridge_paths {
                let helper = std::env::current_exe()?;
                state.set_claude_bridge(Arc::new(
                    crate::claude_bridge::control::BridgeControl::new(
                        settings,
                        capture.clone(),
                        helper,
                    ),
                ));
                std::fs::create_dir_all(&capture)?;
                let watcher_state = state.clone();
                tauri::async_runtime::spawn(async move {
                    watcher_state.watch_claude_capture(capture).await;
                });
            } else {
                eprintln!("Jentrics Claude setup paths unavailable; bridge remains unavailable.");
            }
            state.set_wake_runner(Arc::new(crate::wake::provider::NativeWakeRunner::new(
                search_dirs,
            )));
            let handle = app.handle().clone();
            state.set_publisher(Arc::new(move |snapshot| {
                crate::tray::update(&handle, snapshot);
                if handle.get_webview_window("dashboard").is_some()
                    && handle
                        .emit_to("dashboard", "agent-state-updated", snapshot)
                        .is_err()
                {
                    eprintln!("Jentrics could not publish dashboard state.");
                }
            }));
            app.manage(state.clone());
            crate::tray::install(app)?;
            tauri::async_runtime::spawn(state.run_monitor());
            Ok(())
        })
        .build(tauri::generate_context!())
}

pub fn run() {
    match build_app() {
        Ok(app) => app.run(|_handle, event| {
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        }),
        Err(_) => {
            eprintln!("Jentrics could not initialize its desktop runtime.");
            std::process::exit(1);
        }
    }
}
