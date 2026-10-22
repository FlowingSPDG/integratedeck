//! Surface abstraction: hardware inputs and cell rendering.

mod capabilities;
mod driver;
mod hid;
mod input;
mod manager;
mod update;

pub use capabilities::*;
pub use driver::*;
pub use hid::*;
pub use input::*;
pub use manager::*;
pub use update::CellUpdate;

use async_trait::async_trait;
use ideck_core::{SurfaceId, TouchStripSegment};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SurfaceError {
    #[error("surface not found: {0:?}")]
    NotFound(SurfaceId),
    #[error("{0}")]
    Other(String),
}

/// Render target: mock emulator or physical hardware.
#[async_trait]
// `async_trait` already returns a `#[must_use]` future, so the extra attribute
// it generates is redundant (clippy::double_must_use, new in Rust 1.99).
#[allow(clippy::double_must_use)]
pub trait Surface: Send + Sync {
    fn descriptor(&self) -> &SurfaceDescriptor;
    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError>;

    /// Push dial feedback to a touch strip. Surfaces without a strip ignore it.
    async fn render_touch_strip(
        &self,
        _segments: &[TouchStripSegment],
    ) -> Result<(), SurfaceError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CellId(pub u32);

/// Logical cell address on a surface (row/column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellAddress {
    pub row: u32,
    pub column: u32,
}

impl CellAddress {
    pub fn to_cell_id(&self, columns: u32) -> CellId {
        CellId(self.row * columns + self.column)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SurfaceDescriptor {
    pub id: SurfaceId,
    pub name: String,
    pub capabilities: SurfaceCapabilities,
    pub backend: SurfaceBackend,
}

/// How a surface is backed at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceBackend {
    /// Physical device opened via a [`SurfaceDriverPlugin`].
    Physical {
        driver_id: String,
        device_id: String,
    },
    Mock,
}
