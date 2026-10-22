use std::path::{Path, PathBuf};

use ideck_js_runtime::JsEngine;
use serde_json::Value;
use thiserror::Error;
use tokio::process::Child;
use tracing::{error, warn};

use crate::launch::{
    build_registration_info, close_webview_plugin, default_window_size, spawn_embedded_plugin,
    spawn_native, spawn_node, webview_plugin_host, PluginLaunchStrategy, SdEntryMode,
    WebviewPluginRequest,
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
    Native(Box<Child>),
    Node(Box<Child>),
    Embedded(JsEngine),
    /// HTML plugin rendered in a WebView owned by the application layer.
    Webview,
}

pub struct PluginProcess {
    pub plugin_uuid: String,
    pub plugin_dir: PathBuf,
    pub port: u16,
    runtime: PluginRuntime,
}

impl PluginProcess {
    /// OS process id of a native/Node plugin, if it has one.
    pub fn pid(&self) -> Option<u32> {
        match &self.runtime {
            PluginRuntime::Native(child) | PluginRuntime::Node(child) => child.id(),
            _ => None,
        }
    }

    /// Terminate the plugin process (or WebView) now. Called when a plugin is
    /// unloaded or the app exits: the OS does not reap children for us, and the
    /// broker can outlive the supervisor entry because of task clones.
    pub fn shutdown(&mut self) {
        match &mut self.runtime {
            PluginRuntime::Native(child) => {
                let _ = child.start_kill();
                let _ = child.try_wait();
            }
            PluginRuntime::Node(child) => {
                let _ = child.start_kill();
                let _ = child.try_wait();
            }
            PluginRuntime::Embedded(engine) => {
                let _ = engine;
            }
            PluginRuntime::Webview => {
                close_webview_plugin(&self.plugin_uuid);
            }
        }
    }
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        match &mut self.runtime {
            PluginRuntime::Native(child) => {
                let _ = child.start_kill();
            }
            PluginRuntime::Node(child) => {
                let _ = child.start_kill();
            }
            PluginRuntime::Embedded(engine) => {
                let _ = engine;
            }
            PluginRuntime::Webview => {
                close_webview_plugin(&self.plugin_uuid);
            }
        }
    }
}

impl PluginProcess {
    pub async fn spawn(
        plugin_dir: &Path,
        port: u16,
        devices: Value,
    ) -> Result<Self, PluginLaunchError> {
        let manifest = StreamDeckManifest::load(plugin_dir)?;
        let strategy = PluginLaunchStrategy::detect(&manifest, plugin_dir)?;
        let info = build_registration_info(&manifest.uuid, &manifest.version, devices).to_string();
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
            PluginLaunchStrategy::NodeScript { script } => {
                let child = spawn_node(
                    &script,
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
                    runtime: PluginRuntime::Node(Box::new(child)),
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
                    runtime: PluginRuntime::Native(Box::new(child)),
                })
            }
            PluginLaunchStrategy::HtmlWebView { html } => {
                // Browser plugins need a real DOM, so prefer the installed WebView
                // host. Without one (headless tests, library consumers) keep the
                // previous Boa fallback instead of failing to load the plugin.
                if let Some(host) = webview_plugin_host() {
                    let plugin_uuid = manifest.uuid.clone();
                    let request = WebviewPluginRequest {
                        plugin_uuid: plugin_uuid.clone(),
                        plugin_name: manifest.name.clone(),
                        plugin_dir: plugin_dir.to_path_buf(),
                        html_path: html.clone(),
                        port,
                        register_event: register_event.to_string(),
                        info: info.clone(),
                        window_size: default_window_size(plugin_dir),
                    };
                    if let Err(e) = host.open(request) {
                        error!("WebView SD plugin failed to open: {e:#}");
                        return Err(PluginLaunchError::Other(format!(
                            "webview plugin start failed: {e:#}"
                        )));
                    }
                    return Ok(Self {
                        plugin_uuid,
                        plugin_dir: plugin_dir.to_path_buf(),
                        port,
                        runtime: PluginRuntime::Webview,
                    });
                }

                warn!(
                    "no WebView plugin host installed; running HTML plugin {} in Boa",
                    html.display()
                );
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
