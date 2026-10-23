use tauri::{App, AppHandle, Manager, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tracing::{info, warn};

pub const MAIN_WINDOW_LABEL: &str = "main";
pub const SETTINGS_WINDOW_LABEL: &str = "settings";

/// Create the main window from `tauri.conf.json` (must run during `setup`, not from IPC).
pub fn ensure_main_window(app: &App) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        attach_exit_on_close(&window);
        return Ok(());
    }

    let config = window_config(app, MAIN_WINDOW_LABEL)?;
    let mut profile_reset = false;

    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(80 * attempt as u64));
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                attach_exit_on_close(&window);
                let _ = window.show();
                let _ = window.set_focus();
                info!("main window ready after retry");
                return Ok(());
            }
        }

        match build_window(app.handle(), &config, false) {
            Ok(window) => {
                // `build()` can succeed while the WebView itself fails to start
                // (0x800700AA from a locked profile): the window then has no page
                // and the app would run headless. Give the page a moment to load.
                let mut loaded = false;
                for _ in 0..40 {
                    if window.url().is_ok() {
                        loaded = true;
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                if !loaded {
                    warn!("main window has no page; retrying with a fresh WebView2 profile");
                    let _ = window.close();
                    if !profile_reset && park_webview_profile(app.handle()) {
                        profile_reset = true;
                    }
                    continue;
                }
                attach_exit_on_close(&window);
                let _ = window.show();
                let _ = window.set_focus();
                info!("main window created");
                return Ok(());
            }
            Err(e) => {
                warn!(attempt, "main window build failed: {e}");
                // A killed instance can leave the WebView2 user-data folder
                // locked (0x800700AA). Park it once and start with a clean one;
                // the app's own data lives in %APPDATA% and is unaffected.
                if !profile_reset && is_locked_profile_error(&e) {
                    profile_reset = true;
                    if park_webview_profile(app.handle()) {
                        warn!("retrying with a fresh WebView2 profile");
                    }
                }
            }
        }
    }

    Err(
        "WebView2 の作成に失敗しました。アプリを再起動するか、Microsoft Edge WebView2 Runtime を修復してください。"
            .to_string(),
    )
}

/// Pre-create the settings popup during `setup` on the main thread.
///
/// On Windows, creating a webview inside a **synchronous** IPC command deadlocks WebView2
/// (close button unresponsive). This is therefore called from the main thread after
/// `setup` returns (see `lib.rs`), and the window stays hidden with
/// `skip_taskbar` + `hide()` until the user opens settings.
pub fn ensure_settings_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window(SETTINGS_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    let config = window_config_from_handle(app, SETTINGS_WINDOW_LABEL)?;
    let mut window = None;
    for attempt in 0..2 {
        match build_window(app, &config, true) {
            Ok(built) => {
                window = Some(built);
                break;
            }
            Err(e) => {
                warn!(attempt, "settings window build failed: {e}");
                if attempt == 0 && is_locked_profile_error(&e) {
                    park_webview_profile(app);
                }
            }
        }
    }
    let window = window.ok_or_else(|| "設定ウィンドウの作成に失敗しました".to_string())?;
    attach_hide_on_close(&window);
    window.hide().map_err(|e| e.to_string())?;
    info!("settings window created (hidden)");
    Ok(())
}

/// WebView2 reports "The requested resource is in use" when another process
/// still holds the user-data folder.
fn is_locked_profile_error(error: &str) -> bool {
    error.contains("0x800700AA")
        || error.to_ascii_lowercase().contains("resource is in use")
        || error
            .to_ascii_lowercase()
            .contains("being used by another process")
}

/// Move the locked WebView2 profile aside so the next creation starts clean.
fn park_webview_profile(app: &AppHandle) -> bool {
    let Ok(dir) = app.path().app_local_data_dir() else {
        return false;
    };
    let profile = dir.join("EBWebView");
    if !profile.is_dir() {
        return false;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let parked = dir.join(format!("EBWebView.stale-{stamp}"));
    match std::fs::rename(&profile, &parked) {
        Ok(()) => {
            warn!(parked = %parked.display(), "parked locked WebView2 profile");
            prune_parked_profiles(&dir);
            true
        }
        Err(e) => {
            warn!("could not park the WebView2 profile: {e}");
            false
        }
    }
}

/// Keep the three newest parked profiles, drop the rest.
fn prune_parked_profiles(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut parked: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("EBWebView.stale-"))
                .unwrap_or(false)
        })
        .collect();
    if parked.len() <= 3 {
        return;
    }
    parked.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    });
    for old in parked.iter().rev().skip(3) {
        let _ = std::fs::remove_dir_all(old);
    }
}

/// Show the singleton settings popup. Never creates a webview here (Windows-safe).
pub fn show_settings_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(SETTINGS_WINDOW_LABEL)
        .ok_or_else(|| {
            "設定ウィンドウを準備しています。数秒おいて、もう一度お試しください。".to_string()
        })?;

    window.set_skip_taskbar(false).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.unminimize().ok();
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

fn window_config(app: &App, label: &str) -> Result<tauri::utils::config::WindowConfig, String> {
    window_config_from_handle(app.handle(), label)
}

fn window_config_from_handle(
    app: &AppHandle,
    label: &str,
) -> Result<tauri::utils::config::WindowConfig, String> {
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
    let mut builder = WebviewWindowBuilder::from_config(app, config).map_err(|e| e.to_string())?;

    if start_hidden {
        builder = builder.visible(false).skip_taskbar(true);
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
            let _ = hide_target.set_skip_taskbar(true);
            let _ = hide_target.hide();
        }
    });
}

/// Quit the app when the main window closes.
///
/// The settings window is pre-created and hidden (`attach_hide_on_close`), so closing
/// only the main window would otherwise leave the process running.
fn attach_exit_on_close(window: &WebviewWindow) {
    let app = window.app_handle().clone();
    window.clone().on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            app.exit(0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{is_locked_profile_error, prune_parked_profiles};

    #[test]
    fn recognises_the_locked_profile_error() {
        assert!(is_locked_profile_error(
            "failed to create webview: WebView2 error: WindowsError(Error { code: HRESULT(0x800700AA), message: \"The requested resource is in use.\" })"
        ));
        assert!(is_locked_profile_error(
            "The file is being used by another process"
        ));
        assert!(!is_locked_profile_error("WebView2 runtime not found"));
        assert!(!is_locked_profile_error("missing main window config"));
    }

    #[test]
    fn keeps_only_the_three_newest_parked_profiles() {
        let root = std::env::temp_dir().join(format!("ideck-park-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for index in 0..5 {
            let dir = root.join(format!("EBWebView.stale-{index}"));
            std::fs::create_dir_all(&dir).unwrap();
            // Space the mtimes so the ordering is deterministic.
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::fs::create_dir_all(root.join("EBWebView")).unwrap();

        prune_parked_profiles(&root);

        let mut left: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("EBWebView.stale-"))
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![
                "EBWebView.stale-2",
                "EBWebView.stale-3",
                "EBWebView.stale-4"
            ]
        );
        assert!(root.join("EBWebView").is_dir());

        let _ = std::fs::remove_dir_all(&root);
    }
}
