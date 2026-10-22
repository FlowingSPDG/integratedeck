//! Companion module engine: a JSON-RPC bridge to the Node sidecar.

use std::path::Path;

use serde_json::Value;
use tokio::sync::broadcast;
use tracing::info;

pub use crate::engine::EmbeddedEngine;
#[derive(Debug, Clone, serde::Serialize)]
pub struct HostRequest {
    pub id: String,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostResponse {
    pub id: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct HostEvent {
    #[serde(default)]
    pub event: String,
    #[serde(flatten)]
    pub data: Value,
}

/// In-process embedded JS engine for Companion module compatibility.
pub struct CompModuleRuntime {
    inner: EmbeddedEngine,
}

impl CompModuleRuntime {
    pub async fn spawn(modules_dir: &Path) -> anyhow::Result<Self> {
        let inner = EmbeddedEngine::spawn(modules_dir)?;
        info!(
            "companion module engine started (node={}, root={})",
            crate::engine::resolved_node_binary(),
            modules_dir.display()
        );
        Ok(Self { inner })
    }

    pub async fn request(
        &self,
        method: impl Into<String>,
        params: Value,
    ) -> anyhow::Result<HostResponse> {
        self.inner.request(method, params).await
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<HostEvent> {
        self.inner.subscribe_events()
    }

    pub async fn ping(&self) -> anyhow::Result<Value> {
        self.inner.ping().await
    }

    /// Stop the sidecar (app exit).
    pub async fn shutdown(&self) {
        self.inner.shutdown().await;
    }

    /// False when the sidecar process has gone away.
    pub fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }
}
