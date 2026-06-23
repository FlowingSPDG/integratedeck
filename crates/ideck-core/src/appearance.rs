use serde::{Deserialize, Serialize};

use crate::{ImageFormat, ImagePayload, VisualState};

/// User-defined key appearance (Title / default image), separate from plugin PI settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SlotAppearance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_image: Option<ImagePayload>,
}

impl SlotAppearance {
    pub fn apply_to_visual(&self, visual: &mut VisualState) {
        if let Some(title) = &self.title {
            if !title.is_empty() {
                visual.title = Some(title.clone());
            }
        }
        if visual.image.is_none() {
            visual.image = self.default_image.clone();
        }
    }

    pub fn base_visual(&self) -> VisualState {
        let mut v = VisualState::default();
        self.apply_to_visual(&mut v);
        v
    }
}

/// Decode a data-URL or raw base64 image string into an `ImagePayload`.
pub fn image_from_base64(data: &str, format: ImageFormat) -> Option<ImagePayload> {
    let bytes = decode_image_bytes(data)?;
    Some(ImagePayload {
        format: detect_image_format(&bytes).unwrap_or(format),
        data: bytes,
    })
}

/// Resolve a Stream Deck `setImage` payload: base64/data-URL or relative path under `plugin_dir`.
pub fn image_from_plugin_payload(data: &str, plugin_dir: Option<&std::path::Path>) -> Option<ImagePayload> {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("data:image/") || trimmed.len() > 260 {
        return image_from_base64(trimmed, ImageFormat::Png);
    }
    if looks_like_image_path(trimmed) {
        let path = std::path::Path::new(trimmed);
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            plugin_dir?.join(path)
        };
        let bytes = std::fs::read(&resolved).ok()?;
        if bytes.is_empty() {
            return None;
        }
        return Some(ImagePayload {
            format: detect_image_format(&bytes).unwrap_or(ImageFormat::Png),
            data: bytes,
        });
    }
    image_from_base64(trimmed, ImageFormat::Png)
}

fn looks_like_image_path(s: &str) -> bool {
    s.contains('/') || s.contains('\\') || s.ends_with(".png") || s.ends_with(".jpg") || s.ends_with(".jpeg") || s.ends_with(".bmp")
}

fn decode_image_bytes(data: &str) -> Option<Vec<u8>> {
    let b64 = data
        .strip_prefix("data:image/bmp;base64,")
        .or_else(|| data.strip_prefix("data:image/png;base64,"))
        .or_else(|| data.strip_prefix("data:image/jpeg;base64,"))
        .or_else(|| data.strip_prefix("data:image/jpg;base64,"))
        .unwrap_or(data);
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .or_else(|_| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(b64.trim())
        })
        .ok()
        .filter(|b| !b.is_empty())
}

fn detect_image_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.len() >= 2 && bytes[0] == b'B' && bytes[1] == b'M' {
        Some(ImageFormat::Bmp)
    } else {
        None
    }
}
