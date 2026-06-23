//! Physical surface driver plugin API (Companion `@companion-surface/base` analogue).
//!
//! Each device family implements [`SurfaceDriverPlugin`]. The registry discovers
//! and connects hardware; Orchestrator routes [`SurfaceInput`] regardless of driver.

mod registry;
pub mod streamdeck;

pub use registry::*;
pub use streamdeck::*;

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::{CellUpdate, Surface, SurfaceCapabilities, SurfaceDescriptor, SurfaceError, SurfaceInput};
use ideck_core::SurfaceId;

/// Static metadata for a surface driver plugin (like a Companion surface module manifest).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceDriverManifest {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// A physical device discovered by a driver but not yet connected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredDevice {
    pub driver_id: String,
    pub device_id: String,
    pub label: String,
    pub product: String,
    pub rows: u32,
    pub columns: u32,
}

/// Driver plugin for a family of physical control surfaces.
pub trait SurfaceDriverPlugin: Send + Sync {
    fn manifest(&self) -> SurfaceDriverManifest;
    fn scan(&self) -> Result<Vec<DiscoveredDevice>, SurfaceError>;
    fn connect(
        &self,
        device_id: &str,
        surface_id: SurfaceId,
        label: &str,
    ) -> Result<Arc<dyn PhysicalSurface>, SurfaceError>;
}

/// Connected physical surface with input events and lifecycle hooks.
#[async_trait]
pub trait PhysicalSurface: Surface {
    fn surface_id(&self) -> SurfaceId;
    fn subscribe_inputs(&self) -> broadcast::Receiver<SurfaceInput>;
    /// Background task that polls hardware and publishes [`SurfaceInput`].
    fn spawn_input_loop(self: Arc<Self>) -> JoinHandle<()>;
    fn descriptor(&self) -> &SurfaceDescriptor;
    fn capabilities(&self) -> &SurfaceCapabilities;
    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError>;
    fn as_any(self: Arc<Self>) -> Arc<dyn std::any::Any + Send + Sync>;
}
