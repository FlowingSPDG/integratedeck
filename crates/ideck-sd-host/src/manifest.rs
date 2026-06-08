use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("manifest not found")]
    NotFound,
    #[error("manifest missing required field: {0}")]
    MissingField(&'static str),
}

/// Elgato Stream Deck plugin manifest (`manifest.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct StreamDeckManifest {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "UUID")]
    pub uuid: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(default, rename = "Author")]
    pub author: Option<String>,
    #[serde(rename = "CodePath")]
    pub code_path: String,
    #[serde(default, rename = "CodePathMac")]
    pub code_path_mac: Option<String>,
    #[serde(default, rename = "CodePathWin")]
    pub code_path_win: Option<String>,
    #[serde(default, rename = "Actions")]
    pub actions: Vec<ManifestAction>,
    #[serde(default, rename = "Nodejs")]
    pub nodejs: Option<NodejsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestAction {
    #[serde(default, rename = "UUID")]
    pub uuid: Option<String>,
    #[serde(default, rename = "Name")]
    pub name: Option<String>,
    #[serde(default, rename = "PropertyInspectorPath")]
    pub property_inspector: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodejsConfig {
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(default, rename = "Debug")]
    pub debug: Option<Value>,
}

impl StreamDeckManifest {
    pub fn load(plugin_dir: &Path) -> Result<Self, ManifestError> {
        let path = plugin_dir.join("manifest.json");
        if !path.exists() {
            return Err(ManifestError::NotFound);
        }
        let data = std::fs::read_to_string(&path)?;
        let manifest: Self = serde_json::from_str(&data)?;
        if manifest.uuid.is_empty() {
            return Err(ManifestError::MissingField("UUID"));
        }
        Ok(manifest)
    }

    /// Best-effort read for plugin scanning when strict load is not required.
    pub fn read_summary(plugin_dir: &Path) -> Option<ManifestSummary> {
        let path = plugin_dir.join("manifest.json");
        let data = std::fs::read_to_string(path).ok()?;
        let value: Value = serde_json::from_str(&data).ok()?;
        Some(ManifestSummary {
            name: pick_string(&value, &["Name", "name"])?,
            uuid: pick_string(&value, &["UUID", "Uuid", "uuid"]),
            version: pick_string(&value, &["Version", "version"]),
        })
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

    pub fn action_by_uuid(&self, action_uuid: &str) -> Option<&ManifestAction> {
        self.actions
            .iter()
            .find(|a| a.uuid.as_deref() == Some(action_uuid))
    }
}

/// Minimal manifest fields for UI listing.
#[derive(Debug, Clone)]
pub struct ManifestSummary {
    pub name: String,
    pub uuid: Option<String>,
    pub version: Option<String>,
}

fn pick_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = value.get(*key).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_elgato_uuid_field_names() {
        let json = r#"{
            "Name": "Test Plugin",
            "UUID": "com.example.plugin",
            "Version": "1.0.0",
            "CodePath": "bin/plugin.js",
            "Actions": [
                { "Name": "Action A", "UUID": "com.example.action" },
                { "Name": "Category Only" }
            ]
        }"#;
        let manifest: StreamDeckManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.uuid, "com.example.plugin");
        assert_eq!(manifest.name, "Test Plugin");
        assert_eq!(manifest.actions.len(), 2);
        assert_eq!(
            manifest.actions[0].uuid.as_deref(),
            Some("com.example.action")
        );
        assert!(manifest.actions[1].uuid.is_none());
    }
}
