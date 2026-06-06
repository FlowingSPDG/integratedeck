use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("manifest not found")]
    NotFound,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StreamDeckManifest {
    pub name: String,
    pub uuid: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    pub code_path: String,
    #[serde(default, rename = "CodePathMac")]
    pub code_path_mac: Option<String>,
    #[serde(default, rename = "CodePathWin")]
    pub code_path_win: Option<String>,
    #[serde(default)]
    pub actions: Vec<ManifestAction>,
    #[serde(default, rename = "Nodejs")]
    pub nodejs: Option<NodejsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ManifestAction {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub property_inspector: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodejsConfig {
    pub version: String,
    #[serde(default, rename = "Debug")]
    pub debug: Option<serde_json::Value>,
}

impl StreamDeckManifest {
    pub fn load(plugin_dir: &Path) -> Result<Self, ManifestError> {
        let path = plugin_dir.join("manifest.json");
        if !path.exists() {
            return Err(ManifestError::NotFound);
        }
        let data = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn code_path_for_platform(&self, plugin_dir: &Path) -> PathBuf {
        #[cfg(target_os = "macos")]
        let override_path = self.code_path_mac.as_deref();
        #[cfg(target_os = "windows")]
        let override_path = self.code_path_win.as_deref();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let override_path: Option<&str> = None;

        let rel = override_path.unwrap_or(&self.code_path);
        plugin_dir.join(rel)
    }
}
