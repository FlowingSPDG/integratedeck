//! Surface abstraction: hardware inputs and cell rendering.

mod capabilities;
mod input;
mod manager;
mod satellite;
mod update;

pub use capabilities::*;
pub use input::*;
pub use manager::*;
pub use satellite::*;
pub use update::*;

use serde::{Deserialize, Serialize};
use ideck_core::SurfaceId;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceBackend {
    CompanionSidecar { module_id: String },
    StreamDeckHid,
    Mock,
}
