//! Plugin launch strategy detection and process spawning.

mod embedded;
mod native;
mod node;
mod webview;

use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

use crate::StreamDeckManifest;

#[derive(Debug, Clone)]
pub enum PluginLaunchStrategy {
    /// Simple script run inside the embedded Boa engine.
    JsPlugin {
        script: PathBuf,
    },
    /// Node SDK plugin (`"Nodejs"` in the manifest) run by the real Node runtime.
    NodeScript {
        script: PathBuf,
    },
    NativeBinary {
        exe: PathBuf,
    },
    HtmlWebView {
        html: PathBuf,
    },
}

#[derive(Debug, Error)]
pub enum LaunchStrategyError {
    #[error("manifest: {0}")]
    Manifest(#[from] crate::ManifestError),
    #[error("unsupported code path: {0}")]
    Unsupported(String),
}

impl PluginLaunchStrategy {
    pub fn detect(
        manifest: &StreamDeckManifest,
        plugin_dir: &Path,
    ) -> Result<Self, LaunchStrategyError> {
        let code_path = manifest.resolve_code_path(plugin_dir).ok_or_else(|| {
            LaunchStrategyError::Unsupported(format!(
                "no supported entry found in {}",
                plugin_dir.display()
            ))
        })?;
        let ext = code_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext == "html" || ext == "htm" {
            return Ok(Self::HtmlWebView { html: code_path });
        }

        if matches!(ext.as_str(), "js" | "mjs" | "cjs") {
            if manifest.nodejs.is_some() {
                return Ok(Self::NodeScript { script: code_path });
            }
            return Ok(Self::JsPlugin { script: code_path });
        }

        if manifest.nodejs.is_some() {
            return Ok(Self::NodeScript { script: code_path });
        }

        if code_path.is_file() {
            return Ok(Self::NativeBinary { exe: code_path });
        }

        Err(LaunchStrategyError::Unsupported(
            code_path.display().to_string(),
        ))
    }
}

pub fn build_registration_info(plugin_uuid: &str, version: &str, devices: Value) -> Value {
    let platform = if cfg!(target_os = "macos") {
        "mac"
    } else {
        "windows"
    };
    serde_json::json!({
        "application": {
            "font": "Arial",
            "language": "en",
            "platform": platform,
            "platformVersion": "10.0",
            "version": "7.0.0"
        },
        "plugin": { "uuid": plugin_uuid, "version": version },
        "devicePixelRatio": 1,
        "devices": devices,
        "colors": {
            "buttonMouseOverBackgroundColor": "#464646FF",
            "buttonPressedBackgroundColor": "#303030FF",
            "buttonPressedBorderColor": "#000000FF",
            "buttonPressedTextColor": "#FFFFFFFF",
            "highlightColor": "#0078FFFF"
        }
    })
}

pub use embedded::{spawn_embedded_plugin, SdEntryMode};
pub use native::spawn_native;
pub use node::{node_binary, spawn_node};
pub use webview::{
    close_webview_plugin, default_window_size, install_webview_plugin_host, open_webview_plugin,
    webview_plugin_host, WebviewPluginHost, WebviewPluginRequest,
};
