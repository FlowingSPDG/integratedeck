use image::{imageops, ImageBuffer, Rgba, RgbaImage};
use ab_glyph::{Font, FontArc, Glyph, PxScale, ScaleFont, point};

use crate::{ImageFormat, ImagePayload, TitleAlignment, TitleParams, VisualState};

/// Key buffer dimensions for title layout (matches `elgato_streamdeck` key image size).
#[derive(Debug, Clone, Copy)]
pub struct KeyDisplayHints {
    pub width: u32,
    pub height: u32,
}

impl KeyDisplayHints {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// Merge plugin-driven visual updates without wiping unrelated fields.
pub fn merge_visual(
    base: VisualState,
    patch: &VisualState,
    update_state: bool,
    replace_image: bool,
    replace_title: bool,
) -> VisualState {
    let mut out = base;
    if replace_title {
        out.title = patch.title.clone();
        out.title_params = patch.title_params.clone();
    } else if patch.title.is_some() {
        out.title = patch.title.clone();
    }
    if replace_image || patch.image.is_some() {
        out.image = patch.image.clone();
    }
    if update_state {
        out.state_index = patch.state_index;
    }
    if patch.bgcolor.is_some() {
        out.bgcolor = patch.bgcolor;
    }
    out
}

/// Build a PNG for HID upload: key-sized image + optional title caption.
pub fn compose_key_png(visual: &VisualState, hints: KeyDisplayHints) -> Option<Vec<u8>> {
    let has_title = visual
        .title
        .as_ref()
        .is_some_and(|t| !t.trim().is_empty());
    if visual.image.is_none() && !has_title {
        return None;
    }

    let mut canvas = decode_base_image(visual, hints.width, hints.height)?;

    if has_title {
        draw_title_on_key(
            &mut canvas,
            visual.title.as_deref().unwrap_or(""),
            &visual.title_params,
            hints,
        );
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    canvas.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(buf.into_inner())
}

/// Solid-color key image for showAlert (red) / showOk (green) feedback.
pub fn solid_key_png(width: u32, height: u32, r: u8, g: u8, b: u8) -> ImagePayload {
    let img: RgbaImage = ImageBuffer::from_fn(width, height, |_, _| Rgba([r, g, b, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode solid key png");
    ImagePayload {
        format: ImageFormat::Png,
        data: buf.into_inner(),
    }
}

fn decode_base_image(visual: &VisualState, width: u32, height: u32) -> Option<RgbaImage> {
    if let Some(payload) = &visual.image {
        let img = image::ImageReader::new(std::io::Cursor::new(&payload.data))
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?;
        let rgba = img.to_rgba8();
        if rgba.width() == width && rgba.height() == height {
            return Some(rgba);
        }
        return Some(imageops::resize(
            &rgba,
            width,
            height,
            imageops::FilterType::Lanczos3,
        ));
    }
    Some(ImageBuffer::from_pixel(width, height, Rgba([0, 0, 0, 255])))
}

fn draw_title_on_key(
    canvas: &mut RgbaImage,
    title: &str,
    params: &TitleParams,
    hints: KeyDisplayHints,
) {
    let Some(font) = load_ui_font() else {
        return;
    };

    let width = hints.width;
    let height = hints.height;
    if width == 0 || height == 0 {
        return;
    }

    // Stream Deck SDK default is 12pt; scale with key height (72 px → ~12, 96 px → ~14).
    let font_size = if params.font_size > 0 && params.font_size != 12 {
        params.font_size as f32
    } else {
        (height as f32 * 12.0 / 72.0).clamp(11.0, 16.0)
    };
    let scale = PxScale::from(font_size);
    let scaled = font.as_scaled(scale);

    let line_height = (scaled.ascent() - scaled.descent() + 4.0).ceil() as u32;
    let bar_height = line_height.saturating_add(4).min(height / 2).max(12);

    let max_width = (width as f32 * 0.94).max(1.0);
    let display = truncate_to_width(title.trim(), &scaled, max_width);
    if display.is_empty() {
        return;
    }

    let text_width: f32 = display
        .chars()
        .map(|c| scaled.h_advance(scaled.glyph_id(c)))
        .sum();

    let (bar_top, baseline_y) =
        title_bar_geometry(params, height, bar_height, scaled.ascent());

    let x = match params.alignment {
        TitleAlignment::Left => 4.0,
        TitleAlignment::Right => (width as f32 - text_width - 4.0).max(0.0),
        _ => ((width as f32 - text_width) / 2.0).max(0.0),
    };

    // Opaque bar — device upload is RGB/JPEG; alpha is discarded downstream.
    for py in bar_top..height.min(bar_top + bar_height) {
        for px in 0..width {
            *canvas.get_pixel_mut(px, py) = Rgba([0, 0, 0, 255]);
        }
    }

    let mut cursor_x = x;
    for ch in display.chars() {
        let glyph_id = scaled.glyph_id(ch);
        let glyph = Glyph {
            id: glyph_id,
            scale,
            position: point(cursor_x, baseline_y),
        };
        if let Some(outlined) = scaled.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, alpha| {
                if alpha < 0.05 {
                    return;
                }
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                    return;
                }
                let pixel = canvas.get_pixel_mut(px as u32, py as u32);
                let a = (alpha * 255.0).round() as u8;
                if a > 0 {
                    pixel[0] = 255;
                    pixel[1] = 255;
                    pixel[2] = 255;
                    pixel[3] = 255;
                }
            });
        }
        cursor_x += scaled.h_advance(glyph_id);
    }
}

fn title_bar_geometry(
    params: &TitleParams,
    height: u32,
    bar_height: u32,
    ascent: f32,
) -> (u32, f32) {
    let bar_top = match params.alignment {
        TitleAlignment::Top => 0,
        TitleAlignment::Middle => height.saturating_sub(bar_height) / 2,
        _ => height.saturating_sub(bar_height),
    };
    let baseline_y = bar_top as f32 + ascent + 2.0;
    (bar_top, baseline_y)
}

fn truncate_to_width<F: Font>(text: &str, font: &impl ScaleFont<F>, max_width: f32) -> String {
    let mut width = 0.0f32;
    let mut out = String::new();
    for ch in text.chars() {
        let w = font.h_advance(font.glyph_id(ch));
        if width + w > max_width && !out.is_empty() {
            break;
        }
        width += w;
        out.push(ch);
    }
    if out.chars().count() < text.chars().count() {
        out.push('…');
    }
    out
}

fn load_ui_font() -> Option<FontArc> {
    const CANDIDATES: &[&str] = &[
        "C:\\Windows\\Fonts\\arial.ttf",
        "C:\\Windows\\Fonts\\meiryo.ttc",
        "C:\\Windows\\Fonts\\YuGothR.ttc",
        "C:\\Windows\\Fonts\\msgothic.ttc",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    ];
    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let font = if path.ends_with(".ttc") {
            ab_glyph::FontVec::try_from_vec_and_index(bytes, 0).ok()
        } else {
            ab_glyph::FontVec::try_from_vec(bytes).ok()
        };
        if let Some(font) = font {
            return Some(FontArc::new(font));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_visual_keeps_image_when_only_title_patched() {
        let base = VisualState {
            image: Some(ImagePayload {
                format: ImageFormat::Png,
                data: vec![1, 2, 3],
            }),
            ..Default::default()
        };
        let patch = VisualState {
            title: Some("Hello".into()),
            ..Default::default()
        };
        let merged = merge_visual(base.clone(), &patch, false, false, false);
        assert_eq!(merged.title.as_deref(), Some("Hello"));
        assert_eq!(merged.image, base.image);
    }

    #[test]
    fn merge_visual_set_title_replaces_title() {
        let base = VisualState {
            title: Some("Old".into()),
            ..Default::default()
        };
        let patch = VisualState {
            title: Some("New".into()),
            ..Default::default()
        };
        let merged = merge_visual(base, &patch, false, false, true);
        assert_eq!(merged.title.as_deref(), Some("New"));
    }

    #[test]
    fn merge_visual_set_image_unchanged_keeps_image() {
        let base = VisualState {
            image: Some(ImagePayload {
                format: ImageFormat::Png,
                data: vec![1, 2, 3],
            }),
            ..Default::default()
        };
        let patch = VisualState {
            state_index: 1,
            ..Default::default()
        };
        let merged = merge_visual(base.clone(), &patch, true, false, false);
        assert_eq!(merged.image, base.image);
        assert_eq!(merged.state_index, 1);
    }

    #[test]
    fn compose_key_png_none_without_content() {
        let hints = KeyDisplayHints::new(96, 96);
        assert!(compose_key_png(&VisualState::default(), hints).is_none());
    }

    #[test]
    fn compose_title_renders_visible_pixels_at_bottom() {
        let hints = KeyDisplayHints::new(96, 96);
        let visual = VisualState {
            title: Some("42".into()),
            ..Default::default()
        };
        let png = compose_key_png(&visual, hints).expect("compose title png");
        let img = image::load_from_memory(&png).expect("decode png").to_rgba8();
        let mut white_in_bar = 0u32;
        for y in 72..96 {
            for x in 0..96 {
                let p = img.get_pixel(x, y);
                if p[0] > 200 && p[1] > 200 && p[2] > 200 {
                    white_in_bar += 1;
                }
            }
        }
        assert!(
            white_in_bar > 20,
            "expected white glyph pixels in bottom bar, got {white_in_bar}"
        );
    }
}
