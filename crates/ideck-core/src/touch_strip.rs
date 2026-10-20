//! Rendering for the Stream Deck + touch strip.
//!
//! Each dial owns one segment of the strip. A segment shows the title a plugin
//! set with `setTitle` / `setFeedback`, an optional large value, a horizontal
//! indicator bar (`setFeedback.indicator.value`) and an optional icon image.

use ab_glyph::{point, Font, FontArc, Glyph, PxScale, ScaleFont};
use image::{ImageBuffer, Rgba, RgbaImage};

use crate::visual::{ImageFormat, ImagePayload, VisualState};

/// One dial's slice of the touch strip.
#[derive(Debug, Clone, Default)]
pub struct TouchStripSegment {
    pub index: u32,
    pub visual: VisualState,
}

const BACKGROUND: Rgba<u8> = Rgba([16, 18, 24, 255]);
const DIVIDER: Rgba<u8> = Rgba([44, 48, 58, 255]);
const TITLE_COLOR: Rgba<u8> = Rgba([190, 196, 208, 255]);
const VALUE_COLOR: Rgba<u8> = Rgba([255, 255, 255, 255]);
const INDICATOR_TRACK: Rgba<u8> = Rgba([58, 64, 76, 255]);
const INDICATOR_FILL: Rgba<u8> = Rgba([0, 200, 255, 255]);

/// Render the whole strip as a PNG payload for `write_lcd_fill`.
///
/// `segments` need not be sorted; they are matched to their slot by `index`.
/// Segments beyond the number of zones are ignored.
pub fn render_touch_strip(
    width: u32,
    height: u32,
    segments: &[TouchStripSegment],
    zones: u32,
) -> Option<ImagePayload> {
    if width == 0 || height == 0 || zones == 0 {
        return None;
    }
    let mut canvas: RgbaImage = ImageBuffer::from_pixel(width, height, BACKGROUND);
    let font = load_ui_font();

    let zone_width = width / zones;
    for zone in 0..zones {
        let x0 = zone * zone_width;
        let x1 = if zone + 1 == zones {
            width
        } else {
            x0 + zone_width
        };
        draw_divider(&mut canvas, x0, height);

        let Some(segment) = segments.iter().find(|s| s.index == zone) else {
            continue;
        };
        draw_segment(&mut canvas, x0, x1, height, &segment.visual, font.as_ref());
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    canvas.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(ImagePayload {
        format: ImageFormat::Png,
        data: buf.into_inner(),
    })
}

fn draw_divider(canvas: &mut RgbaImage, x: u32, height: u32) {
    if x == 0 || x >= canvas.width() {
        return;
    }
    for y in 4..height.saturating_sub(4) {
        canvas.put_pixel(x, y, DIVIDER);
    }
}

fn draw_segment(
    canvas: &mut RgbaImage,
    x0: u32,
    x1: u32,
    height: u32,
    visual: &VisualState,
    font: Option<&FontArc>,
) {
    let pad = 10u32;
    let inner_left = x0 + pad;
    let inner_right = x1.saturating_sub(pad);
    if inner_right <= inner_left {
        return;
    }
    let inner_width = inner_right - inner_left;

    // Icon, when the plugin supplied one (left aligned, square).
    let mut text_left = inner_left;
    if let Some(image) = &visual.image {
        if let Some(icon) = decode_square(image, height.saturating_sub(pad * 2)) {
            let size = icon.height();
            let top = (height - size) / 2;
            for (ix, iy, px) in icon.enumerate_pixels() {
                let dx = inner_left + ix;
                let dy = top + iy;
                if dx < x1 && dy < height && px[3] > 0 {
                    canvas.put_pixel(dx, dy, *px);
                }
            }
            text_left = inner_left + size + pad;
        }
    }

    let title = visual.title.as_deref().unwrap_or("").trim();
    let indicator = visual.indicator;
    let has_indicator = indicator.is_some();

    if let Some(font) = font {
        // Size text against the zone, not just the strip height: a dial on the
        // 1424x280 Studio panel has far more room than one on an 800x100 strip.
        let zone_width = (x1 - x0) as f32;
        let value_px = (height as f32 * 0.46)
            .min(zone_width * 0.34)
            .clamp(16.0, 130.0);
        let title_px = (value_px * 0.34).clamp(9.0, 34.0);
        let value_scale = PxScale::from(value_px);
        let title_scale = PxScale::from(title_px);

        // Value line: the whole title when it is short, otherwise the first line.
        let (value_line, title_line) = split_dial_text(title);
        let value_y = if title_line.is_some() {
            height as f32 * 0.52
        } else {
            height as f32 * 0.66
        };
        if let Some(value) = value_line {
            if !value.is_empty() {
                draw_text(
                    canvas,
                    &value,
                    text_left,
                    inner_right,
                    value_y,
                    value_scale,
                    font,
                    VALUE_COLOR,
                );
            }
        }
        if let Some(title_line) = title_line {
            draw_text(
                canvas,
                &title_line,
                text_left,
                inner_right,
                height as f32 * 0.24,
                title_scale,
                font,
                TITLE_COLOR,
            );
        }
    }

    if let Some(value) = indicator {
        let track_top = height.saturating_sub(14);
        let track_height = 5u32;
        fill_rect(
            canvas,
            inner_left,
            track_top,
            inner_right,
            track_top + track_height,
            INDICATOR_TRACK,
        );
        let ratio = value.min(100) as f32 / 100.0;
        let filled = (inner_width as f32 * ratio).round() as u32;
        if filled > 0 && has_indicator {
            fill_rect(
                canvas,
                inner_left,
                track_top,
                inner_left + filled,
                track_top + track_height,
                INDICATOR_FILL,
            );
        }
    }
}

/// Split a dial label into (value, secondary title).
///
/// Plugins usually write `"42"`, but preset layouts often use two lines
/// (`"Volume\n42"`); the last line is the prominent value.
fn split_dial_text(text: &str) -> (Option<String>, Option<String>) {
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    match lines.len() {
        0 => (None, None),
        1 => (Some(lines.remove(0).trim().to_string()), None),
        _ => {
            let value = lines.pop().map(|l| l.trim().to_string());
            let title = Some(lines.join(" "));
            (value, title)
        }
    }
}

fn decode_square(payload: &ImagePayload, max: u32) -> Option<RgbaImage> {
    if max == 0 {
        return None;
    }
    let img = image::ImageReader::new(std::io::Cursor::new(&payload.data))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .to_rgba8();
    let side = img.width().min(img.height()).min(max);
    if side == 0 {
        return None;
    }
    Some(image::imageops::resize(
        &img,
        side,
        side,
        image::imageops::FilterType::Lanczos3,
    ))
}

fn fill_rect(canvas: &mut RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32, color: Rgba<u8>) {
    let x1 = x1.min(canvas.width());
    let y1 = y1.min(canvas.height());
    for y in y0..y1 {
        for x in x0..x1 {
            canvas.put_pixel(x, y, color);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    canvas: &mut RgbaImage,
    text: &str,
    left: u32,
    right: u32,
    baseline_y: f32,
    scale: PxScale,
    font: &FontArc,
    color: Rgba<u8>,
) {
    let scaled = font.as_scaled(scale);
    let max_width = right.saturating_sub(left) as f32;
    if max_width <= 0.0 {
        return;
    }
    let display = truncate_to_width(text, &scaled, max_width);
    if display.is_empty() {
        return;
    }

    let mut cursor_x = left as f32;
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
                if px < 0 || py < 0 || px >= canvas.width() as i32 || py >= canvas.height() as i32 {
                    return;
                }
                let pixel = canvas.get_pixel_mut(px as u32, py as u32);
                let a = (alpha * 255.0).round() as u8;
                if a > 0 {
                    *pixel = color;
                }
            });
        }
        cursor_x += scaled.h_advance(glyph_id);
    }
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
        "/System/Library/Fonts/Supplemental/Arial.ttf",
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

    fn decode(payload: &ImagePayload) -> RgbaImage {
        image::ImageReader::new(std::io::Cursor::new(&payload.data))
            .with_guessed_format()
            .expect("format")
            .decode()
            .expect("decode")
            .to_rgba8()
    }

    #[test]
    fn renders_configured_strip_size() {
        let payload = render_touch_strip(800, 100, &[], 4).expect("strip");
        let img = decode(&payload);
        assert_eq!(img.width(), 800);
        assert_eq!(img.height(), 100);
    }

    #[test]
    fn draws_divider_between_zones_and_nothing_outside() {
        let payload = render_touch_strip(800, 100, &[], 4).expect("strip");
        let img = decode(&payload);
        // 200 px zones -> divider columns at 200 and 400.
        assert_ne!(img.get_pixel(200, 50), &BACKGROUND);
        assert_ne!(img.get_pixel(400, 50), &BACKGROUND);
        // Column 0 is the strip edge and stays background.
        assert_eq!(img.get_pixel(0, 50), &BACKGROUND);
    }

    #[test]
    fn draws_indicator_bar_for_the_matching_zone() {
        let visual = VisualState {
            indicator: Some(50),
            ..Default::default()
        };
        let segments = vec![TouchStripSegment { index: 1, visual }];
        let payload = render_touch_strip(800, 100, &segments, 4).expect("strip");
        let img = decode(&payload);
        // Zone 1 starts at 200; the bar sits near the bottom and is filled for
        // half of the inner width.
        let bar_y = 100 - 12;
        let filled = img.get_pixel(240, bar_y);
        let empty = img.get_pixel(360, bar_y);
        assert_ne!(filled, &BACKGROUND);
        assert_eq!(empty, &INDICATOR_TRACK);
    }

    #[test]
    fn ignores_segments_beyond_the_zone_count() {
        let segments = vec![TouchStripSegment {
            index: 9,
            visual: VisualState {
                indicator: Some(100),
                ..Default::default()
            },
        }];
        let payload = render_touch_strip(800, 100, &segments, 4).expect("strip");
        let img = decode(&payload);
        // Zone 9 does not exist, so no zone may show its indicator.
        assert_eq!(img.get_pixel(450, 88), &BACKGROUND);
    }

    #[test]
    fn splits_two_line_labels_into_value_and_title() {
        let (value, title) = split_dial_text("Volume\n42");
        assert_eq!(value.as_deref(), Some("42"));
        assert_eq!(title.as_deref(), Some("Volume"));

        let (value, title) = split_dial_text("42");
        assert_eq!(value.as_deref(), Some("42"));
        assert!(title.is_none());
    }
}
