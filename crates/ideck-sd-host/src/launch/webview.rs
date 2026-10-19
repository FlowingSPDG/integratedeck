//! Host abstraction for HTML/browser Stream Deck plugins.
//!
//! HTML plugins (`CodePath` ending in `.html`/`.htm`) need a real DOM and `window`,
//! so they cannot run inside the embedded Boa engine (it fails with errors such as
//! `must be declarative environment` as soon as the page touches browser globals).
//!
//! `ideck-sd-host` stays UI-toolkit agnostic: the desktop shell installs a
//! [`WebviewPluginHost`] once at startup, and launching an HTML plugin hands the
//! request below to it. The shell creates a WebView window, loads the plugin page
//! and injects a shim that calls the page's `connectElgatoStreamDeckSocket` with
//! the broker port — the same contract the Elgato Stream Deck application uses.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde_json::Value;

/// Everything the UI layer needs to open (or reload) the WebView for an HTML plugin.
#[derive(Debug, Clone)]
pub struct WebviewPluginRequest {
    /// Plugin UUID from `manifest.json`.
    pub plugin_uuid: String,
    /// Display name from `manifest.json` (may be blank for some plugins).
    pub plugin_name: String,
    /// Plugin bundle directory; also the root the host must expose to the page.
    pub plugin_dir: PathBuf,
    /// Absolute path of the HTML entry point (`CodePath`).
    pub html_path: PathBuf,
    /// Broker WebSocket port the page has to connect to.
    pub port: u16,
    /// Registration event name (`registerPlugin`).
    pub register_event: String,
    /// JSON-encoded `info` payload passed to `connectElgatoStreamDeckSocket`.
    pub info: String,
    /// Optional window size from the manifest `DefaultWindowSize`.
    pub window_size: Option<(u32, u32)>,
}

impl WebviewPluginRequest {
    /// `actionInfo` for a plugin window: empty object, as sent by Stream Deck.
    pub fn action_info(&self) -> &'static str {
        "{}"
    }
}

/// Window backend used to host HTML plugins.
///
/// The application layer implements this on top of its WebView toolkit. Plain
/// library use (tests, headless tooling) can leave it uninstalled; HTML plugins
/// then fall back to the embedded engine so existing behaviour is preserved.
pub trait WebviewPluginHost: Send + Sync + 'static {
    /// Load `request.html_path` and connect it to the broker on `request.port`.
    fn open(&self, request: WebviewPluginRequest) -> anyhow::Result<()>;

    /// Called when the plugin is unloaded; the host should hide/unload its page.
    fn close(&self, plugin_uuid: &str) -> anyhow::Result<()> {
        let _ = plugin_uuid;
        Ok(())
    }

    /// Bring the plugin window back to the foreground, if it is still hosting it.
    fn focus(&self, plugin_uuid: &str) -> anyhow::Result<()> {
        let _ = plugin_uuid;
        Ok(())
    }
}

static HOST: OnceLock<Arc<dyn WebviewPluginHost>> = OnceLock::new();

/// Installs the process-wide WebView host. Only the first call wins.
pub fn install_webview_plugin_host(host: Arc<dyn WebviewPluginHost>) -> Result<(), String> {
    HOST.set(host)
        .map_err(|_| "WebView plugin host is already installed".to_string())
}

/// Returns the installed WebView host, if any.
pub fn webview_plugin_host() -> Option<Arc<dyn WebviewPluginHost>> {
    HOST.get().cloned()
}

/// Opens the WebView for an HTML plugin (no-op error when no host is installed).
pub fn open_webview_plugin(request: WebviewPluginRequest) -> anyhow::Result<()> {
    let host = HOST
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no WebView plugin host installed"))?;
    host.open(request)
}

/// Releases the WebView hosting `plugin_uuid` (ignored when no host is installed).
pub fn close_webview_plugin(plugin_uuid: &str) {
    if let Some(host) = HOST.get() {
        if let Err(e) = host.close(plugin_uuid) {
            tracing::warn!("failed to close WebView for {plugin_uuid}: {e:#}");
        }
    }
}

/// Best-effort read of `DefaultWindowSize` from `manifest.json`.
///
/// `manifest.rs` only models the fields the broker needs; the window size is a
/// pure UI concern, so it is read here without touching manifest parsing.
pub fn default_window_size(plugin_dir: &Path) -> Option<(u32, u32)> {
    let raw = std::fs::read_to_string(plugin_dir.join("manifest.json")).ok()?;
    let raw = raw.trim_start_matches('\u{feff}');
    let manifest: Value = serde_json::from_str(raw).ok()?;
    let size = manifest.get("DefaultWindowSize")?.as_array()?;
    let width = size.first()?.as_u64()?;
    let height = size.get(1)?.as_u64()?;
    // Logical pixels; ignore nonsense values instead of creating an unusable window.
    if !(200..=10_000).contains(&width) || !(200..=10_000).contains(&height) {
        return None;
    }
    Some((width as u32, height as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_plugin_dir(name: &str, manifest: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ideck-sd-webview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        dir
    }

    #[test]
    fn reads_default_window_size() {
        let dir = temp_plugin_dir(
            "size",
            r#"{"UUID":"com.example.test","CodePath":"app.html","DefaultWindowSize":[1320,816]}"#,
        );
        assert_eq!(default_window_size(&dir), Some((1320, 816)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ignores_missing_or_invalid_window_size() {
        let dir = temp_plugin_dir(
            "no-size",
            r#"{"UUID":"com.example.test","CodePath":"app.html"}"#,
        );
        assert_eq!(default_window_size(&dir), None);
        let _ = std::fs::remove_dir_all(dir);

        let dir = temp_plugin_dir(
            "bad-size",
            r#"{"UUID":"com.example.test","CodePath":"app.html","DefaultWindowSize":[10,10]}"#,
        );
        assert_eq!(default_window_size(&dir), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tolerates_utf8_bom() {
        let dir = temp_plugin_dir(
            "bom",
            "\u{feff}{\"UUID\":\"com.example.test\",\"DefaultWindowSize\":[800,600]}",
        );
        assert_eq!(default_window_size(&dir), Some((800, 600)));
        let _ = std::fs::remove_dir_all(dir);
    }
}
