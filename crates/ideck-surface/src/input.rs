use serde::{Deserialize, Serialize};

use crate::CellAddress;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SurfaceInput {
    KeyDown {
        address: CellAddress,
    },
    KeyUp {
        address: CellAddress,
    },
    EncoderRotate {
        index: u32,
        ticks: i32,
        pressed: bool,
    },
    EncoderPress {
        index: u32,
        pressed: bool,
    },
    Touch {
        address: CellAddress,
        x: f32,
        y: f32,
    },
    /// Touch strip tap / long press. `index` is the dial the tap landed on, so
    /// drivers can resolve it without knowing the surface geometry again.
    TouchTap {
        index: u32,
        x: u32,
        y: u32,
        hold: bool,
    },
    /// Touch strip swipe. Elgato does not forward swipes to plugins, so the app
    /// uses them for page navigation; the raw coordinates are kept for logging.
    TouchSwipe {
        start_x: u32,
        start_y: u32,
        end_x: u32,
        end_y: u32,
    },
    PageChange {
        page_index: u32,
    },
    Nfc {
        uid: Vec<u8>,
    },
}
