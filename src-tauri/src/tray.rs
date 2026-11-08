use crate::{
    dashboard::request_open_dashboard, model::DashboardState, state::AppState, tray_policy,
};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

pub fn request_quit(app: &tauri::AppHandle) {
    let handle = app.clone();
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        let state = state.inner().clone();
        tauri::async_runtime::spawn(async move {
            state.stop().await;
            handle.exit(0);
        });
    } else {
        handle.exit(0);
    }
}
fn menu(
    app: &tauri::AppHandle,
    view: &tray_policy::TrayView,
) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let open = MenuItem::with_id(app, "open", "Open dashboard", true, None::<&str>)?;
    let claude = MenuItem::with_id(
        app,
        "claude-status",
        &view.claude_status,
        false,
        None::<&str>,
    )?;
    let claude_reset =
        MenuItem::with_id(app, "claude-reset", &view.claude_reset, false, None::<&str>)?;
    let codex = MenuItem::with_id(app, "codex-status", &view.codex_status, false, None::<&str>)?;
    let codex_reset =
        MenuItem::with_id(app, "codex-reset", &view.codex_reset, false, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let pause = MenuItem::with_id(
        app,
        "pause",
        view.pause_label,
        view.pause_enabled,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &open,
            &claude,
            &claude_reset,
            &codex,
            &codex_reset,
            &refresh,
            &pause,
            &settings,
            &quit,
        ],
    )
}
pub fn update(app: &tauri::AppHandle, snapshot: &DashboardState) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let state = app.state::<Arc<AppState>>();
    let view = tray_policy::view(snapshot, &state.monitor_settings(), crate::state::now_ms());
    let updated = menu(app, &view)
        .and_then(|menu| tray.set_menu(Some(menu)))
        .and_then(|_| tray.set_tooltip(Some(&view.tooltip)));
    if updated.is_err() {
        state.record_diagnostic(crate::diagnostics::EventCode::TrayUpdateFailed, 0, 1);
        eprintln!("Jentrics tray update unavailable.");
    }
}
pub fn install(app: &tauri::App) -> Result<(), tauri::Error> {
    let state = app.state::<Arc<AppState>>();
    let view = tray_policy::view(
        &state.snapshot(),
        &state.monitor_settings(),
        crate::state::now_ms(),
    );
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip(&view.tooltip)
        .menu(&menu(app.handle(), &view)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => request_open_dashboard(app),
            "refresh" => {
                let state = app.state::<Arc<AppState>>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    state.refresh().await;
                });
            }
            "pause" => {
                let state = app.state::<Arc<AppState>>();
                if let Some(settings) = tray_policy::pause_action(&state.monitor_settings()) {
                    if state.save_monitor_settings(settings).is_err() {
                        eprintln!("Jentrics monitor settings unavailable.");
                    }
                }
            }
            "settings" => {
                app.state::<Arc<AppState>>().request_settings();
                request_open_dashboard(app);
                if app.emit_to("dashboard", "settings-requested", ()).is_err() {
                    eprintln!(
                        "Jentrics Settings notification unavailable; request remains pending."
                    );
                }
            }
            "quit" => request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                request_open_dashboard(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    state.record_diagnostic(crate::diagnostics::EventCode::TrayReady, 0, 1);
    Ok(())
}
