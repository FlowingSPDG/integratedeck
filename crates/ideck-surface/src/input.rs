use serde::{Deserialize, Serialize};

use crate::CellAddress;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SurfaceInput {
    KeyDown { address: CellAddress },
    KeyUp { address: CellAddress },
    EncoderRotate {
        index: u32,
        ticks: i32,
        pressed: bool,
    },
    EncoderPress { index: u32, pressed: bool },
    Touch { address: CellAddress, x: f32, y: f32 },
    PageChange { page_index: u32 },
}
