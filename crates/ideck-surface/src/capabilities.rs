use serde::{Deserialize, Serialize};

/// Describes what a surface can do and how to emulate Stream Deck layouts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceCapabilities {
    pub rows: u32,
    pub columns: u32,
    pub key_width_px: u32,
    pub key_height_px: u32,
    #[serde(default)]
    pub encoders: u32,
    #[serde(default)]
    pub has_lcd_strip: bool,
    #[serde(default)]
    pub emulation_profile: EmulationProfile,
}

impl SurfaceCapabilities {
    pub fn streamdeck_mk2() -> Self {
        Self {
            rows: 3,
            columns: 5,
            key_width_px: 72,
            key_height_px: 72,
            encoders: 0,
            has_lcd_strip: false,
            emulation_profile: EmulationProfile::StreamDeckMk2,
        }
    }

    pub fn mock_3x5() -> Self {
        Self::streamdeck_mk2()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmulationProfile {
    #[default]
    StreamDeckMk2,
    StreamDeckOriginal,
    StreamDeckPlus,
    StreamDeckXl,
    Custom,
}
