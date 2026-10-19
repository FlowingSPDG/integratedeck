//! Transport-neutral Stream Deck Studio protocol pieces.
//!
//! USB HID and the legacy LAN transport use the same 1024-byte output
//! commands and 512-byte input reports. Only the transport differs.
//!
//! Layouts are from a real Studio (firmware 2.01.002, USB `0fd9:00aa`);
//! see productiondeck `docs/studio-protocol.md`.

use crate::{CellAddress, SurfaceInput};
use ideck_core::{ImagePayload, VisualState};

pub const OUTPUT_REPORT_LEN: usize = 1024;
pub const INPUT_REPORT_LEN: usize = 512;
pub const ROWS: u32 = 2;
pub const COLS: u32 = 16;
pub const KEYS: usize = (ROWS * COLS) as usize;
pub const ENCODERS: usize = 2;
pub const KEY_WIDTH: u32 = 144;
pub const KEY_HEIGHT: u32 = 112;

/// Parsed `0x08` unit-info feature report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StudioUnitInfo {
    pub rows: u32,
    pub columns: u32,
    pub key_width: u32,
    pub key_height: u32,
    pub lcd_width: u32,
    pub lcd_height: u32,
}

impl Default for StudioUnitInfo {
    fn default() -> Self {
        Self {
            rows: ROWS,
            columns: COLS,
            key_width: KEY_WIDTH,
            key_height: KEY_HEIGHT,
            lcd_width: 1424,
            lcd_height: 280,
        }
    }
}

/// `0x08` unit-info response:
/// `08 rows cols key_w16 key_h16 lcd_w16 lcd_h16 ...`.
pub fn parse_unit_info(report: &[u8]) -> Option<StudioUnitInfo> {
    if report.len() < 12 || report[0] != 0x08 {
        return None;
    }
    Some(StudioUnitInfo {
        rows: u32::from(report[1]),
        columns: u32::from(report[2]),
        key_width: u32::from(u16::from_le_bytes([report[3], report[4]])),
        key_height: u32::from(u16::from_le_bytes([report[5], report[6]])),
        lcd_width: u32::from(u16::from_le_bytes([report[7], report[8]])),
        lcd_height: u32::from(u16::from_le_bytes([report[9], report[10]])),
    })
}

/// `0x06` serial response: `06 len ASCII...`.
pub fn parse_serial(report: &[u8]) -> Option<String> {
    if report.len() < 2 || report[0] != 0x06 {
        return None;
    }
    let len = usize::from(report[1]).min(report.len().saturating_sub(2));
    String::from_utf8(report[2..2 + len].to_vec()).ok()
}

fn base_report() -> Vec<u8> {
    vec![0u8; OUTPUT_REPORT_LEN]
}

pub fn key_color_packet(key: u8, rgb: (u8, u8, u8)) -> Vec<u8> {
    let mut packet = base_report();
    packet[0] = 0x03;
    packet[1] = 0x06;
    packet[2] = key;
    packet[3] = rgb.0;
    packet[4] = rgb.1;
    packet[5] = rgb.2;
    packet
}

pub fn brightness_packet(value: u8) -> Vec<u8> {
    let mut packet = base_report();
    packet[0] = 0x03;
    packet[1] = 0x08;
    packet[2] = value;
    packet
}

pub fn encoder_led_packet(encoder: u8, rgb: (u8, u8, u8)) -> Vec<u8> {
    let mut packet = base_report();
    packet[0] = 0x02;
    packet[1] = 0x10;
    packet[2] = encoder;
    packet[3] = rgb.0;
    packet[4] = rgb.1;
    packet[5] = rgb.2;
    packet
}

pub fn encoder_ring_packet(encoder: u8, colors: &[(u8, u8, u8); 24]) -> Vec<u8> {
    let mut packet = base_report();
    packet[0] = 0x02;
    packet[1] = 0x0f;
    packet[2] = encoder;
    for (index, (r, g, b)) in colors.iter().enumerate() {
        let offset = 3 + index * 3;
        packet[offset] = *r;
        packet[offset + 1] = *g;
        packet[offset + 2] = *b;
    }
    packet
}

pub fn key_image_packets(key: u8, jpeg: &[u8]) -> Vec<Vec<u8>> {
    const HEADER: usize = 8;
    const CHUNK: usize = OUTPUT_REPORT_LEN - HEADER;
    let mut packets = Vec::new();
    for (index, chunk) in jpeg.chunks(CHUNK).enumerate() {
        let last = (index + 1) * CHUNK >= jpeg.len();
        let mut packet = base_report();
        packet[0] = 0x02;
        packet[1] = 0x07;
        packet[2] = key;
        packet[3] = u8::from(last);
        packet[4..6].copy_from_slice(&(chunk.len() as u16).to_le_bytes());
        packet[6..8].copy_from_slice(&(index as u16).to_le_bytes());
        packet[HEADER..HEADER + chunk.len()].copy_from_slice(chunk);
        packets.push(packet);
    }
    packets
}

/// Encode a slot image as a 144x112 JPEG, filling transparency with the slot
/// background colour.
pub fn encode_key_jpeg(image: &ImagePayload, visual: &VisualState) -> Option<Vec<u8>> {
    let decoded = image::load_from_memory(&image.data).ok()?;
    let mut canvas = image::RgbaImage::new(KEY_WIDTH, KEY_HEIGHT);
    if let Some(color) = visual.bgcolor {
        for pixel in canvas.pixels_mut() {
            *pixel = image::Rgba([color.r, color.g, color.b, 255]);
        }
    }
    let resized = decoded.resize_to_fill(
        KEY_WIDTH,
        KEY_HEIGHT,
        image::imageops::FilterType::Triangle,
    );
    image::imageops::overlay(&mut canvas, &resized.to_rgba8(), 0, 0);
    let mut jpeg = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90);
    encoder
        .encode_image(&image::DynamicImage::ImageRgba8(canvas))
        .ok()?;
    Some(jpeg)
}

/// Fullscreen LCD panel image (`02 0c`).
pub fn lcd_image_packets(x: u16, y: u16, w: u16, h: u16, jpeg: &[u8]) -> Vec<Vec<u8>> {
    const HEADER: usize = 16;
    const CHUNK: usize = OUTPUT_REPORT_LEN - HEADER;
    let mut packets = Vec::new();
    for (index, chunk) in jpeg.chunks(CHUNK).enumerate() {
        let last = (index + 1) * CHUNK >= jpeg.len();
        let mut packet = base_report();
        packet[0] = 0x02;
        packet[1] = 0x0c;
        packet[2..4].copy_from_slice(&x.to_le_bytes());
        packet[4..6].copy_from_slice(&y.to_le_bytes());
        packet[6..8].copy_from_slice(&w.to_le_bytes());
        packet[8..10].copy_from_slice(&h.to_le_bytes());
        packet[10] = u8::from(last);
        packet[11..13].copy_from_slice(&(index as u16).to_le_bytes());
        packet[13..15].copy_from_slice(&(chunk.len() as u16).to_le_bytes());
        packet[HEADER..HEADER + chunk.len()].copy_from_slice(chunk);
        packets.push(packet);
    }
    packets
}

/// Decode key/encoder/NFC input from a 512-byte report.
pub fn decode_input(
    report: &[u8],
    previous_keys: &mut [bool; KEYS],
    previous_encoders: &mut [bool; ENCODERS],
) -> Vec<SurfaceInput> {
    if report.len() < 5 || report[0] != 0x01 {
        return Vec::new();
    }
    let len = usize::from(u16::from_le_bytes([report[2], report[3]]));
    let mut updates = Vec::new();
    match report[1] {
        0x00 => {
            for (index, previous) in previous_keys
                .iter_mut()
                .take(len.min(KEYS))
                .enumerate()
            {
                let pressed = report.get(4 + index).copied().unwrap_or(0) != 0;
                if pressed == *previous {
                    continue;
                }
                *previous = pressed;
                let address = CellAddress {
                    row: (index / COLS as usize) as u32,
                    column: (index % COLS as usize) as u32,
                };
                updates.push(if pressed {
                    SurfaceInput::KeyDown { address }
                } else {
                    SurfaceInput::KeyUp { address }
                });
            }
        }
        0x03 => {
            let subtype = report.get(4).copied().unwrap_or(0);
            for index in 0..ENCODERS {
                let value = report.get(5 + index).copied().unwrap_or(0);
                if subtype == 0 {
                    let pressed = value != 0;
                    if previous_encoders[index] == pressed {
                        continue;
                    }
                    previous_encoders[index] = pressed;
                    updates.push(SurfaceInput::EncoderPress {
                        index: index as u32,
                        pressed,
                    });
                } else if subtype == 1 {
                    let delta = value as i8;
                    if delta != 0 {
                        updates.push(SurfaceInput::EncoderRotate {
                            index: index as u32,
                            ticks: i32::from(delta),
                            pressed: false,
                        });
                    }
                }
            }
        }
        0x04 => {
            let uid = report
                .get(4..4 + len.min(report.len().saturating_sub(4)))
                .unwrap_or_default()
                .to_vec();
            updates.push(SurfaceInput::Nfc { uid });
        }
        _ => {}
    }
    updates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_captured_unit_info() {
        let report = [
            0x08, 0x02, 0x10, 0x90, 0x00, 0x70, 0x00, 0x90, 0x05, 0x18, 0x01, 0x02, 0x00, 0x00,
            0x00, 0x04, 0xff,
        ];
        let info = parse_unit_info(&report).unwrap();
        assert_eq!(info.rows, 2);
        assert_eq!(info.columns, 16);
        assert_eq!(info.key_width, 144);
        assert_eq!(info.key_height, 112);
        assert_eq!(info.lcd_width, 1424);
        assert_eq!(info.lcd_height, 280);
    }

    #[test]
    fn decodes_keys_encoders_and_nfc() {
        let mut keys = [false; KEYS];
        let mut encoders = [false; ENCODERS];

        let mut key = [0u8; INPUT_REPORT_LEN];
        key[0] = 0x01;
        key[2] = 2;
        key[4] = 1;
        assert!(matches!(
            decode_input(&key, &mut keys, &mut encoders).as_slice(),
            [SurfaceInput::KeyDown { address: CellAddress { row: 0, column: 0 } }]
        ));
        key[4] = 0;
        assert!(matches!(
            decode_input(&key, &mut keys, &mut encoders).as_slice(),
            [SurfaceInput::KeyUp { address: CellAddress { row: 0, column: 0 } }]
        ));

        let mut enc = [0u8; INPUT_REPORT_LEN];
        enc[0] = 0x01;
        enc[1] = 0x03;
        enc[2] = 2;
        enc[4] = 0;
        enc[5] = 1;
        assert!(matches!(
            decode_input(&enc, &mut keys, &mut encoders).as_slice(),
            [SurfaceInput::EncoderPress {
                index: 0,
                pressed: true
            }]
        ));
        enc[4] = 1;
        enc[5] = 2;
        assert!(matches!(
            decode_input(&enc, &mut keys, &mut encoders).as_slice(),
            [SurfaceInput::EncoderRotate {
                index: 0,
                ticks: 2,
                ..
            }]
        ));

        let mut nfc = [0u8; INPUT_REPORT_LEN];
        nfc[0] = 0x01;
        nfc[1] = 0x04;
        nfc[2] = 4;
        nfc[4..8].copy_from_slice(&[1, 2, 3, 4]);
        assert!(matches!(
            decode_input(&nfc, &mut keys, &mut encoders).as_slice(),
            [SurfaceInput::Nfc { .. }]
        ));
    }

    #[test]
    fn chunks_key_images() {
        let packets = key_image_packets(3, &vec![0xab; 2000]);
        assert_eq!(packets.len(), 2);
        assert_eq!(&packets[0][..4], &[0x02, 0x07, 3, 0]);
        assert_eq!(u16::from_le_bytes([packets[0][4], packets[0][5]]), 1016);
        assert_eq!(packets[1][3], 1);
        assert_eq!(u16::from_le_bytes([packets[1][4], packets[1][5]]), 984);
    }
}
