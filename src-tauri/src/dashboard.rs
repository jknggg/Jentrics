use crate::state::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub fn request_open_dashboard(app: &AppHandle) {
    if open_dashboard(app).is_err() {
        eprintln!("Jentrics could not open its dashboard.");
        app.exit(1);
    }
}

pub fn open_dashboard(app: &AppHandle) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("dashboard") {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
    } else {
        WebviewWindowBuilder::new(app, "dashboard", WebviewUrl::App("index.html".into()))
            .title("Jentrics")
            .inner_size(960.0, 720.0)
            .min_inner_size(660.0, 500.0)
            .build()?
            // Full view is opaque; mini mode explicitly enables the glass background.
            .set_background_color(Some(tauri::window::Color(255, 255, 255, 255)))?;
    }
    let state = app.state::<Arc<AppState>>().inner().clone();
    tauri::async_runtime::spawn(async move {
        state.refresh().await;
    });
    Ok(())
}
