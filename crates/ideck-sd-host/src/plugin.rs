use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde_json::{json, Value};
use thiserror::Error;
use tokio::process::Child;
use tokio::process::Command;

use crate::StreamDeckManifest;

#[derive(Debug, Error)]
pub enum PluginLaunchError {
    #[error("manifest: {0}")]
    Manifest(#[from] crate::ManifestError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported code path: {0}")]
    UnsupportedCodePath(String),
}

pub struct PluginProcess {
    pub plugin_uuid: String,
    pub plugin_dir: PathBuf,
    pub child: Child,
    pub port: u16,
}

impl PluginProcess {
    pub async fn spawn_node(
        plugin_dir: &Path,
        port: u16,
        node_binary: &Path,
    ) -> Result<Self, PluginLaunchError> {
        let manifest = StreamDeckManifest::load(plugin_dir)?;
        let code_path = manifest.code_path_for_platform(plugin_dir);

        let register_event = "registerPlugin";
        let info = json!({
            "application": {
                "font": "Arial",
                "language": "en",
                "platform": std::env::consts::OS,
                "platformVersion": "1.0",
                "version": "7.0.0"
            },
            "plugin": {
                "uuid": manifest.uuid,
                "version": manifest.version
            },
            "devices": [{
                "id": "integratedeck-virtual-1",
                "name": "Integratedeck Virtual",
                "size": { "columns": 5, "rows": 3 },
                "type": 0
            }]
        });

        let child = if code_path.extension().map(|e| e == "js").unwrap_or(false)
            || manifest.nodejs.is_some()
        {
            Command::new(node_binary)
                .arg(&code_path)
                .arg("-port")
                .arg(port.to_string())
                .arg("-pluginUUID")
                .arg(&manifest.uuid)
                .arg("-registerEvent")
                .arg(register_event)
                .arg("-info")
                .arg(info.to_string())
                .current_dir(plugin_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?
        } else if code_path.is_file() {
            Command::new(&code_path)
                .arg("-port")
                .arg(port.to_string())
                .arg("-pluginUUID")
                .arg(&manifest.uuid)
                .arg("-registerEvent")
                .arg(register_event)
                .arg("-info")
                .arg(info.to_string())
                .current_dir(plugin_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?
        } else {
            return Err(PluginLaunchError::UnsupportedCodePath(
                code_path.display().to_string(),
            ));
        };

        Ok(Self {
            plugin_uuid: manifest.uuid,
            plugin_dir: plugin_dir.to_path_buf(),
            child,
            port,
        })
    }
}

pub fn build_registration_info(plugin_uuid: &str, version: &str) -> Value {
    json!({
        "application": {
            "font": "Arial",
            "language": "en",
            "platform": std::env::consts::OS,
            "platformVersion": "1.0",
            "version": "7.0.0"
        },
        "plugin": { "uuid": plugin_uuid, "version": version },
        "devices": [{
            "id": "integratedeck-virtual-1",
            "name": "Integratedeck Virtual",
            "size": { "columns": 5, "rows": 3 },
            "type": 0
        }]
    })
}
