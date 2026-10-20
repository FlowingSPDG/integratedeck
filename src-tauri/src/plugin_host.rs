//! WebView host for HTML/browser Stream Deck plugins.
//!
//! HTML plugins (`manifest.json` `CodePath` → `app.html`) need a real DOM, so they
//! cannot run in the embedded Boa engine. This module owns the Tauri side of the
//! `ideck_sd_host::WebviewPluginHost` contract:
//!
//! 1. `ensure_plugin_host_window` creates a hidden `plugin-host` WebviewWindow during
//!    `setup`. Windows WebView2 deadlocks when a webview is created from an IPC
//!    command (see `windows.rs`), so the window is created once and reused.
//! 2. When an HTML plugin is loaded, the broker port plus registration data are
//!    encoded into the page URL and the window navigates to the plugin's HTML file
//!    through the `asset:` protocol.
//! 3. `assets/plugin_host_shim.js` is injected into every document of the window. It
//!    calls the page's `connectElgatoStreamDeckSocket` with the broker port (the same
//!    contract the Elgato Stream Deck app uses) or installs a fallback bridge that
//!    registers the plugin and forwards incoming events.
//!
//! Multiple HTML plugins can be hosted at once from a small pre-created pool of
//! hidden windows (WebView2 cannot be created safely from an IPC command).

use std::sync::{Arc, Mutex};

use ideck_sd_host::{WebviewPluginHost, WebviewPluginRequest};
use tauri::{
    App, AppHandle, LogicalSize, Manager, State, Url, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::info;

pub const PLUGIN_HOST_WINDOW_LABEL: &str = "plugin-host";
const PLUGIN_HOST_WINDOW_COUNT: usize = 4;

pub fn plugin_host_window_label(index: usize) -> String {
    if index == 0 {
        PLUGIN_HOST_WINDOW_LABEL.to_string()
    } else {
        format!("{PLUGIN_HOST_WINDOW_LABEL}-{index}")
    }
}

const SHIM: &str = include_str!("../assets/plugin_host_shim.js");
const DEFAULT_WIDTH: f64 = 960.0;
const DEFAULT_HEIGHT: f64 = 620.0;
const MIN_WIDTH: f64 = 320.0;
const MIN_HEIGHT: f64 = 240.0;
const CONFIG_QUERY_KEY: &str = "__ideckSd";

/// Creates the reusable hidden plugin window pool during `setup`.
pub fn ensure_plugin_host_window(app: &App) -> Result<(), String> {
    for index in 0..PLUGIN_HOST_WINDOW_COUNT {
        let label = plugin_host_window_label(index);
        if app.get_webview_window(&label).is_some() {
            continue;
        }

        let blank = Url::parse("about:blank").map_err(|e| e.to_string())?;
        let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(blank))
            .title("integratedeck plugin")
            .inner_size(DEFAULT_WIDTH, DEFAULT_HEIGHT)
            .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
            .visible(false)
            .skip_taskbar(true)
            .initialization_script(SHIM);

        #[cfg(debug_assertions)]
        let builder = builder.devtools(true);

        let window = builder
            .build()
            .map_err(|e| format!("failed to create plugin host window {label}: {e}"))?;
        attach_hide_on_close(&window);
    }
    info!("plugin host window pool created (hidden)");
    Ok(())
}

/// Installs the process-wide WebView plugin host used by `ideck-sd-host`.
pub fn install(app: &AppHandle) -> Result<Arc<PluginHost>, String> {
    let host = Arc::new(PluginHost {
        app: app.clone(),
        active: Mutex::new(vec![None; PLUGIN_HOST_WINDOW_COUNT]),
    });
    ideck_sd_host::install_webview_plugin_host(host.clone())?;
    Ok(host)
}

/// Tauri implementation of [`WebviewPluginHost`].
pub struct PluginHost {
    app: AppHandle,
    /// Plugin UUID per pool window, or `None` when the window is free.
    active: Mutex<Vec<Option<String>>>,
}

impl PluginHost {
    fn open_impl(&self, request: WebviewPluginRequest) -> anyhow::Result<()> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let slot = active
            .iter()
            .position(|slot| slot.as_deref() == Some(request.plugin_uuid.as_str()))
            .or_else(|| active.iter().position(|slot| slot.is_none()))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "plugin host window pool is full ({} windows); unload an HTML plugin",
                    PLUGIN_HOST_WINDOW_COUNT
                )
            })?;
        let window = self.window_for(slot)?;

        // Plugins can live outside the configured asset scope (e.g. a dev checkout);
        // allow their bundle directory for the asset protocol before navigating.
        self.app
            .asset_protocol_scope()
            .allow_directory(&request.plugin_dir, true)
            .map_err(|e| {
                anyhow::anyhow!("asset scope for {}: {e}", request.plugin_dir.display())
            })?;

        let url = plugin_url(&request)?;
        let (width, height) = request
            .window_size
            .map(|(w, h)| (w as f64, h as f64))
            .unwrap_or((DEFAULT_WIDTH, DEFAULT_HEIGHT));
        let show_window = request.window_size.is_some();

        window.set_size(LogicalSize::new(width, height))?;
        window.set_title(&window_title(&request))?;
        window.set_skip_taskbar(false)?;
        window.navigate(url)?;
        if show_window {
            window.show()?;
            window.set_focus()?;
        } else {
            // Headless HTML plugins (no DefaultWindowSize) often have a
            // script-only body; keep the window hidden instead of showing an
            // empty subwindow. `focus_plugin_host_window` can reveal it.
            window.hide()?;
        }

        active[slot] = Some(request.plugin_uuid.clone());
        info!(
            plugin = %request.plugin_uuid,
            slot,
            port = request.port,
            html = %request.html_path.display(),
            "plugin host window opened"
        );
        Ok(())
    }

    fn close_impl(&self, plugin_uuid: &str) -> anyhow::Result<()> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(slot) = active
            .iter()
            .position(|slot| slot.as_deref() == Some(plugin_uuid))
        else {
            return Ok(());
        };

        if let Some(window) = self.app.get_webview_window(&plugin_host_window_label(slot)) {
            let _ = window.set_skip_taskbar(true);
            let _ = window.hide();
            // Drop the page (and its WebSocket/timers) until the next HTML plugin.
            let _ = window.navigate(Url::parse("about:blank")?);
        }
        active[slot] = None;
        info!(plugin = plugin_uuid, slot, "plugin host window closed");
        Ok(())
    }

    /// Shows the plugin window. Returns `false` when it is not hosting `plugin_uuid`.
    pub fn focus_window(&self, plugin_uuid: Option<&str>) -> Result<bool, String> {
        let active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let slot = match plugin_uuid {
            Some(wanted) => active
                .iter()
                .position(|slot| slot.as_deref() == Some(wanted)),
            None => active.iter().position(|slot| slot.is_some()),
        };
        let Some(slot) = slot else {
            return Ok(false);
        };

        let window = self.window_for(slot).map_err(|e| e.to_string())?;
        window.set_skip_taskbar(false).map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        window.unminimize().ok();
        window.set_focus().map_err(|e| e.to_string())?;
        Ok(true)
    }

    fn window_for(&self, slot: usize) -> anyhow::Result<WebviewWindow> {
        self.app
            .get_webview_window(&plugin_host_window_label(slot))
            .ok_or_else(|| anyhow::anyhow!("plugin host window {slot} is not initialized"))
    }
}

impl WebviewPluginHost for PluginHost {
    fn open(&self, request: WebviewPluginRequest) -> anyhow::Result<()> {
        self.open_impl(request)
    }

    fn close(&self, plugin_uuid: &str) -> anyhow::Result<()> {
        self.close_impl(plugin_uuid)
    }

    fn focus(&self, plugin_uuid: &str) -> anyhow::Result<()> {
        let _ = self.focus_window(Some(plugin_uuid));
        Ok(())
    }
}

/// Reopens the plugin host window (e.g. after the user closed it).
#[tauri::command]
pub fn focus_plugin_host_window(
    state: State<'_, Arc<PluginHost>>,
    plugin_uuid: Option<String>,
) -> Result<bool, String> {
    state.focus_window(plugin_uuid.as_deref())
}

fn window_title(request: &WebviewPluginRequest) -> String {
    let name = request.plugin_name.trim();
    if name.is_empty() {
        format!("{} — integratedeck", request.plugin_uuid)
    } else {
        format!("{name} — integratedeck")
    }
}

/// Builds `asset:` URL + `__ideckSd` config for the plugin page.
fn plugin_url(request: &WebviewPluginRequest) -> anyhow::Result<Url> {
    let config = serde_json::json!({
        "port": request.port,
        "pluginUUID": request.plugin_uuid,
        "registerEvent": request.register_event,
        "pluginDir": request.plugin_dir.to_string_lossy(),
        "info": request.info,
        "actionInfo": request.action_info(),
    });
    let encoded_path = encode_path_segments(&request.html_path.to_string_lossy());
    let encoded_config = encode_uri_component(&config.to_string());

    // Mirrors `@tauri-apps/api/core` `convertFileSrc` for the `asset` protocol.
    let base = if cfg!(any(windows, target_os = "android")) {
        "http://asset.localhost/"
    } else {
        "asset://localhost/"
    };
    Url::parse(&format!(
        "{base}{encoded_path}?{CONFIG_QUERY_KEY}={encoded_config}"
    ))
    .map_err(Into::into)
}

/// Encode each path segment separately, keeping `/` separators so the browser
/// resolves relative URLs (`libs/js/stream-deck.js`) inside the plugin bundle.
fn encode_path_segments(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .map(encode_uri_component)
        .collect::<Vec<_>>()
        .join("/")
}

/// Equivalent of JavaScript `encodeURIComponent`.
fn encode_uri_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(byte as char),
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

fn attach_hide_on_close(window: &WebviewWindow) {
    let hide_target = window.clone();
    window.clone().on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = hide_target.set_skip_taskbar(true);
            let _ = hide_target.hide();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn request() -> WebviewPluginRequest {
        WebviewPluginRequest {
            plugin_uuid: "com.elgato.tutorial".into(),
            plugin_name: " ".into(),
            plugin_dir: PathBuf::from(
                r"C:\Users\Flowing\AppData\Roaming\Elgato\StreamDeck\Plugins\com.elgato.tutorial.sdPlugin",
            ),
            html_path: PathBuf::from(
                r"C:\Users\Flowing\AppData\Roaming\Elgato\StreamDeck\Plugins\com.elgato.tutorial.sdPlugin\app.html",
            ),
            port: 51234,
            register_event: "registerPlugin".into(),
            info: r#"{"application":{"language":"en"},"plugin":{"uuid":"com.elgato.tutorial"}}"#
                .into(),
            window_size: Some((1320, 816)),
        }
    }

    #[test]
    fn encodes_like_encode_uri_component() {
        assert_eq!(
            encode_uri_component("abcXYZ019-_.!~*'()"),
            "abcXYZ019-_.!~*'()"
        );
        assert_eq!(
            encode_uri_component("C:\\Program Files\\x.html"),
            "C%3A%5CProgram%20Files%5Cx.html"
        );
        assert_eq!(encode_uri_component("a+b/c?d=é"), "a%2Bb%2Fc%3Fd%3D%C3%A9");
    }

    #[test]
    fn keeps_path_segments_for_relative_urls() {
        assert_eq!(
            encode_path_segments(r"C:\plugins\com.elgato.tutorial.sdPlugin\app.html"),
            "C%3A/plugins/com.elgato.tutorial.sdPlugin/app.html"
        );
    }

    #[test]
    fn builds_asset_url_with_config_query() {
        let url = plugin_url(&request()).unwrap();
        let expected_prefix = if cfg!(windows) {
            "http://asset.localhost/"
        } else {
            "asset://localhost/"
        };
        assert!(url.as_str().starts_with(expected_prefix));
        assert!(url.path().ends_with("app.html"));
        assert!(url.query().unwrap_or_default().contains(CONFIG_QUERY_KEY));
        assert!(url.as_str().contains("pluginUUID"));
    }

    #[test]
    fn falls_back_to_uuid_for_blank_name() {
        assert_eq!(
            window_title(&request()),
            "com.elgato.tutorial — integratedeck"
        );
    }
}
