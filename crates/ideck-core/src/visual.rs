use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Unified visual output for a slot (from SD setImage/setTitle or Companion feedback).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VisualState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ImagePayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub title_params: TitleParams,
    #[serde(default)]
    pub state_index: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bgcolor: Option<Color>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variable_overlays: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImagePayload {
    pub format: ImageFormat,
    #[serde(with = "serde_bytes_base64")]
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TitleParams {
    #[serde(default)]
    pub alignment: TitleAlignment,
    #[serde(default = "default_font_size")]
    pub font_size: u8,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_style: Option<String>,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub show_title: bool,
}

fn default_font_size() -> u8 {
    12
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleAlignment {
    Top,
    #[default]
    Bottom,
    Middle,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Parse Stream Deck SDK `titleParams` from a setTitle payload object.
pub fn title_params_from_json(v: &serde_json::Value) -> TitleParams {
    let mut params = TitleParams::default();
    if let Some(size) = v.get("fontSize").and_then(|n| n.as_u64()) {
        params.font_size = size.clamp(6, 24) as u8;
    }
    if let Some(family) = v.get("fontFamily").and_then(|s| s.as_str()) {
        if !family.is_empty() {
            params.font_family = Some(family.to_string());
        }
    }
    if let Some(style) = v.get("fontStyle").and_then(|s| s.as_str()) {
        if !style.is_empty() {
            params.font_style = Some(style.to_string());
        }
    }
    if let Some(underline) = v.get("underline").and_then(|b| b.as_bool()) {
        params.underline = underline;
    }
    if let Some(show) = v.get("showTitle").and_then(|b| b.as_bool()) {
        params.show_title = show;
    }
    if let Some(align) = v.get("alignment").and_then(|s| s.as_str()) {
        params.alignment = match align.to_lowercase().as_str() {
            "top" => TitleAlignment::Top,
            "bottom" => TitleAlignment::Bottom,
            "left" => TitleAlignment::Left,
            "right" => TitleAlignment::Right,
            _ => TitleAlignment::Middle,
        };
    }
    params
}

mod serde_bytes_base64 {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        STANDARD
            .decode(s)
            .map_err(serde::de::Error::custom)
    }
}

// Re-export base64 only for the module above — add base64 to ideck-core