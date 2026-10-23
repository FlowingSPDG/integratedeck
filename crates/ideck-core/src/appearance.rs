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

/// Decode a data URL, rasterizing SVG icons to PNG so they can be sent to a
/// Stream Deck device (which only accepts bitmap key images).
pub fn image_from_data_url(data: &str, size: u32) -> Option<ImagePayload> {
    let (meta, payload) = data.trim().split_once(',')?;
    if meta.contains("svg") && meta.contains("base64") {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload.trim())
            .ok()?;
        return rasterize_svg(&bytes, size);
    }
    image_from_base64(data, ImageFormat::Png)
}

/// SVG options with the system font database loaded once.
///
/// Without fonts, plugin SVGs that draw labels lossily drop their text (usvg
/// warns `No match for 'sans-serif' font-family`) and keys render unlabelled.
fn svg_options() -> &'static resvg::usvg::Options<'static> {
    static OPTIONS: std::sync::OnceLock<resvg::usvg::Options<'static>> = std::sync::OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut options = resvg::usvg::Options::default();
        let mut fontdb = resvg::usvg::fontdb::Database::new();
        fontdb.load_system_fonts();
        options.fontdb = std::sync::Arc::new(fontdb);
        options
    })
}

fn rasterize_svg(bytes: &[u8], size: u32) -> Option<ImagePayload> {
    let options = svg_options();
    let tree = resvg::usvg::Tree::from_data(bytes, options).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let source = tree.size();
    let scale = (size as f32 / source.width()).min(size as f32 / source.height());
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let png = pixmap.encode_png().ok()?;
    Some(ImagePayload {
        format: ImageFormat::Png,
        data: png,
    })
}

/// Resolve a Stream Deck `setImage` payload: base64/data-URL or relative path under `plugin_dir`.
pub fn image_from_plugin_payload(
    data: &str,
    plugin_dir: Option<&std::path::Path>,
) -> Option<ImagePayload> {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("data:image/") {
        // Plugins commonly send SVG; the device needs a bitmap.
        return image_from_data_url(trimmed, 144);
    }
    if trimmed.len() > 260 {
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
    s.contains('/')
        || s.contains('\\')
        || s.ends_with(".png")
        || s.ends_with(".jpg")
        || s.ends_with(".jpeg")
        || s.ends_with(".bmp")
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
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(b64.trim()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_payload_rasterizes_svg() {
        // Plugins commonly send SVG in setImage; it must not be dropped.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144"><rect width="144" height="144" fill="#3366ff"/></svg>"##;
        use base64::Engine;
        let url = format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        );
        let image = image_from_plugin_payload(&url, None).expect("rasterized svg");
        assert_eq!(image.format, ImageFormat::Png);
        assert!(image.data.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn rasterizes_svg_icon_to_png() {
        use base64::Engine;
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><rect width="24" height="24" fill="red"/></svg>"#;
        let url = format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        );
        let image = image_from_data_url(&url, 96).expect("rasterized icon");
        assert_eq!(image.format, ImageFormat::Png);
        assert!(image.data.starts_with(&[0x89, b'P', b'N', b'G']));
        let decoded = image::load_from_memory(&image.data).expect("png");
        assert_eq!((decoded.width(), decoded.height()), (96, 96));
    }
}
