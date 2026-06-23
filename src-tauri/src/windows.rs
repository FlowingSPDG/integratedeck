use tauri::{App, AppHandle, Manager, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tracing::{info, warn};

pub const MAIN_WINDOW_LABEL: &str = "main";
pub const SETTINGS_WINDOW_LABEL: &str = "settings";

/// Create the main window from `tauri.conf.json` (must run during `setup`, not from IPC).
pub fn ensure_main_window(app: &App) -> Result<(), String> {
    if app.get_webview_window(MAIN_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    let config = window_config(app, MAIN_WINDOW_LABEL)?;

    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(80 * attempt as u64));
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                let _ = window.show();
                let _ = window.set_focus();
                info!("main window ready after retry");
                return Ok(());
            }
        }

        match build_window(app.handle(), &config, false) {
            Ok(window) => {
                let _ = window.show();
                let _ = window.set_focus();
                info!("main window created");
                return Ok(());
            }
            Err(e) => warn!(attempt, "main window build failed: {e}"),
        }
    }

    Err(
        "WebView2 の作成に失敗しました。アプリを再起動するか、Microsoft Edge WebView2 Runtime を修復してください。"
            .to_string(),
    )
}

/// Pre-create the settings popup during `setup`.
///
/// On Windows, creating a webview window inside a **synchronous** IPC command deadlocks
/// WebView2 (blank window, close button unresponsive). Windows must be created on the
/// main thread during setup, then shown/hidden via `show_settings_window`.
pub fn ensure_settings_window(app: &App) -> Result<(), String> {
    if app.get_webview_window(SETTINGS_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    let config = window_config(app, SETTINGS_WINDOW_LABEL)?;
    let window = build_window(app.handle(), &config, true)?;
    attach_hide_on_close(&window);
    info!("settings window created (hidden)");
    Ok(())
}

/// Show the singleton settings popup. Never creates a webview here (Windows-safe).
pub fn show_settings_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(SETTINGS_WINDOW_LABEL)
        .ok_or_else(|| {
            "設定ウィンドウが初期化されていません。アプリを再起動してください。".to_string()
        })?;

    window.show().map_err(|e| e.to_string())?;
    window.unminimize().ok();
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

fn window_config(app: &App, label: &str) -> Result<tauri::utils::config::WindowConfig, String> {
    app.config()
        .app
        .windows
        .iter()
        .find(|w| w.label == label)
        .cloned()
        .ok_or_else(|| format!("missing {label} window config"))
}

fn build_window(
    app: &AppHandle,
    config: &tauri::utils::config::WindowConfig,
    start_hidden: bool,
) -> Result<WebviewWindow, String> {
    let mut builder =
        WebviewWindowBuilder::from_config(app, config).map_err(|e| e.to_string())?;

    if start_hidden {
        builder = builder.visible(false);
    }

    #[cfg(debug_assertions)]
    {
        builder = builder.devtools(true);
    }

    builder.build().map_err(|e| e.to_string())
}

fn attach_hide_on_close(window: &WebviewWindow) {
    let hide_target = window.clone();
    window.clone().on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = hide_target.hide();
        }
    });
}
