//! Plugin launch strategy detection and process spawning.

mod html;
mod native;
mod node;

use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

use crate::StreamDeckManifest;

#[derive(Debug, Clone)]
pub enum PluginLaunchStrategy {
    NodeProcess { script: PathBuf },
    NativeBinary { exe: PathBuf },
    HtmlWebView { html: PathBuf },
}

#[derive(Debug, Error)]
pub enum LaunchStrategyError {
    #[error("manifest: {0}")]
    Manifest(#[from] crate::ManifestError),
    #[error("unsupported code path: {0}")]
    Unsupported(String),
}

impl PluginLaunchStrategy {
    pub fn detect(manifest: &StreamDeckManifest, plugin_dir: &Path) -> Result<Self, LaunchStrategyError> {
        let code_path = manifest.code_path_for_platform(plugin_dir);
        let ext = code_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext == "html" || ext == "htm" {
            return Ok(Self::HtmlWebView { html: code_path });
        }

        if ext == "js" || manifest.nodejs.is_some() {
            return Ok(Self::NodeProcess { script: code_path });
        }

        if code_path.is_file() {
            return Ok(Self::NativeBinary { exe: code_path });
        }

        Err(LaunchStrategyError::Unsupported(code_path.display().to_string()))
    }
}

pub fn build_registration_info(
    plugin_uuid: &str,
    version: &str,
    devices: Value,
) -> Value {
    serde_json::json!({
        "application": {
            "font": "Arial",
            "language": "en",
            "platform": std::env::consts::OS,
            "platformVersion": "1.0",
            "version": "7.0.0"
        },
        "plugin": { "uuid": plugin_uuid, "version": version },
        "devices": devices
    })
}

pub use html::{spawn_html_host, HtmlPluginHandle};
pub use native::spawn_native;
pub use node::spawn_node;
