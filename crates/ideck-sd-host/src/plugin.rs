use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;
use tokio::process::Child;

use crate::launch::{
    build_registration_info, spawn_html_host, spawn_native, spawn_node, HtmlPluginHandle,
    PluginLaunchStrategy,
};
use crate::StreamDeckManifest;

#[derive(Debug, Error)]
pub enum PluginLaunchError {
    #[error("manifest: {0}")]
    Manifest(#[from] crate::ManifestError),
    #[error("launch: {0}")]
    Launch(#[from] crate::launch::LaunchStrategyError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

pub struct PluginProcess {
    pub plugin_uuid: String,
    pub plugin_dir: PathBuf,
    pub child: Child,
    pub port: u16,
    _html: Option<HtmlPluginHandle>,
}

impl PluginProcess {
    pub async fn spawn(
        plugin_dir: &Path,
        port: u16,
        node_binary: &Path,
        devices: Value,
        html_host_script: Option<&Path>,
    ) -> Result<Self, PluginLaunchError> {
        let manifest = StreamDeckManifest::load(plugin_dir)?;
        let strategy = PluginLaunchStrategy::detect(&manifest, plugin_dir)?;
        let info =
            build_registration_info(&manifest.uuid, &manifest.version, devices).to_string();
        let register_event = "registerPlugin";

        match strategy {
            PluginLaunchStrategy::NodeProcess { script } => {
                let child = spawn_node(
                    &script,
                    plugin_dir,
                    port,
                    &manifest.uuid,
                    register_event,
                    &info,
                    node_binary,
                )
                .await?;
                Ok(Self {
                    plugin_uuid: manifest.uuid,
                    plugin_dir: plugin_dir.to_path_buf(),
                    child,
                    port,
                    _html: None,
                })
            }
            PluginLaunchStrategy::NativeBinary { exe } => {
                let child = spawn_native(
                    &exe,
                    plugin_dir,
                    port,
                    &manifest.uuid,
                    register_event,
                    &info,
                )
                .await?;
                Ok(Self {
                    plugin_uuid: manifest.uuid,
                    plugin_dir: plugin_dir.to_path_buf(),
                    child,
                    port,
                    _html: None,
                })
            }
            PluginLaunchStrategy::HtmlWebView { html } => {
                let host_script = html_host_script.ok_or_else(|| {
                    PluginLaunchError::Other("html host script path required".into())
                })?;
                let handle = spawn_html_host(
                    &html,
                    plugin_dir,
                    port,
                    &manifest.uuid,
                    &info,
                    node_binary,
                    host_script,
                )
                .await?;
                let HtmlPluginHandle { child } = handle;
                Ok(Self {
                    plugin_uuid: manifest.uuid,
                    plugin_dir: plugin_dir.to_path_buf(),
                    child,
                    port,
                    _html: None,
                })
            }
        }
    }
}
