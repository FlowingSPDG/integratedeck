use std::path::{Path, PathBuf};

use ideck_js_runtime::JsEngine;
use serde_json::Value;
use thiserror::Error;
use tokio::process::Child;
use tracing::error;

use crate::launch::{
    build_registration_info, spawn_embedded_plugin, spawn_native, PluginLaunchStrategy,
    SdEntryMode,
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

enum PluginRuntime {
    Native(Child),
    Embedded(JsEngine),
}

pub struct PluginProcess {
    pub plugin_uuid: String,
    pub plugin_dir: PathBuf,
    pub port: u16,
    runtime: PluginRuntime,
}

impl PluginProcess {
    pub async fn spawn(
        plugin_dir: &Path,
        port: u16,
        devices: Value,
    ) -> Result<Self, PluginLaunchError> {
        let manifest = StreamDeckManifest::load(plugin_dir)?;
        let strategy = PluginLaunchStrategy::detect(&manifest, plugin_dir)?;
        let info =
            build_registration_info(&manifest.uuid, &manifest.version, devices).to_string();
        let register_event = "registerPlugin";

        match strategy {
            PluginLaunchStrategy::JsPlugin { script } => {
                let engine = spawn_embedded_plugin(
                    plugin_dir,
                    &script,
                    SdEntryMode::Script,
                    port,
                    &manifest.uuid,
                    register_event,
                    &info,
                )
                .map_err(|e| {
                    error!("embedded SD plugin failed to start: {e:#}");
                    PluginLaunchError::Other(format!("embedded plugin start failed: {e:#}"))
                })?;
                Ok(Self {
                    plugin_uuid: manifest.uuid,
                    plugin_dir: plugin_dir.to_path_buf(),
                    port,
                    runtime: PluginRuntime::Embedded(engine),
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
                    port,
                    runtime: PluginRuntime::Native(child),
                })
            }
            PluginLaunchStrategy::HtmlWebView { html } => {
                let engine = spawn_embedded_plugin(
                    plugin_dir,
                    &html,
                    SdEntryMode::Html,
                    port,
                    &manifest.uuid,
                    register_event,
                    &info,
                )
                .map_err(|e| {
                    error!("embedded SD HTML plugin failed to start: {e:#}");
                    PluginLaunchError::Other(format!("embedded html plugin start failed: {e:#}"))
                })?;
                Ok(Self {
                    plugin_uuid: manifest.uuid,
                    plugin_dir: plugin_dir.to_path_buf(),
                    port,
                    runtime: PluginRuntime::Embedded(engine),
                })
            }
        }
    }
}
