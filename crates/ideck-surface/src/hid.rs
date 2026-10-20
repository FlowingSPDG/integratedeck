use std::io::Cursor;
use std::sync::Arc;

use elgato_streamdeck::asynchronous::AsyncStreamDeck;
use elgato_streamdeck::images::convert_image_with_format;
use elgato_streamdeck::info::Kind;
use elgato_streamdeck::{list_devices, new_hidapi, refresh_device_list, DeviceStateUpdate};
use ideck_core::KeyDisplayHints;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::ImageReader;
use image::{ColorType, DynamicImage};
use tokio::sync::broadcast;
use tracing::warn;

use crate::{
    capabilities::EmulationProfile, CellAddress, CellUpdate, SurfaceBackend, SurfaceCapabilities,
    SurfaceDescriptor, SurfaceError, SurfaceInput,
};
use ideck_core::SurfaceId;

/// Discovered physical Stream Deck (not yet connected).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HidDeviceDescriptor {
    pub serial: String,
    pub kind: String,
    pub product: String,
    pub rows: u32,
    pub columns: u32,
    pub key_count: u8,
}

pub fn scan_hid_devices() -> Result<Vec<HidDeviceDescriptor>, String> {
    let mut hid = new_hidapi().map_err(|e| e.to_string())?;
    refresh_device_list(&mut hid).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (kind, serial) in list_devices(&hid) {
        if !kind.is_visual() {
            continue;
        }
        let product = kind_label(kind);
        out.push(HidDeviceDescriptor {
            serial,
            kind: kind_id(kind),
            product,
            rows: kind.row_count() as u32,
            columns: kind.column_count() as u32,
            key_count: kind.key_count(),
        });
    }
    out.sort_by(|a, b| a.product.cmp(&b.product).then(a.serial.cmp(&b.serial)));
    Ok(out)
}

pub fn parse_kind_id(id: &str) -> Option<Kind> {
    match id {
        "original" => Some(Kind::Original),
        "original_v2" => Some(Kind::OriginalV2),
        "mini" => Some(Kind::Mini),
        "xl" => Some(Kind::Xl),
        "xl_v2" => Some(Kind::XlV2),
        "mk2" => Some(Kind::Mk2),
        "mk2_scissor" => Some(Kind::Mk2Scissor),
        "mini_mk2" => Some(Kind::MiniMk2),
        "mini_discord" => Some(Kind::MiniDiscord),
        "neo" => Some(Kind::Neo),
        "pedal" => Some(Kind::Pedal),
        "plus" => Some(Kind::Plus),
        "plus_xl" => Some(Kind::PlusXl),
        "mini_mk2_module" => Some(Kind::MiniMk2Module),
        "mk2_module" => Some(Kind::Mk2Module),
        "xl_v2_module" => Some(Kind::XlV2Module),
        _ => None,
    }
}

pub fn kind_id(kind: Kind) -> String {
    match kind {
        Kind::Original => "original",
        Kind::OriginalV2 => "original_v2",
        Kind::Mini => "mini",
        Kind::Xl => "xl",
        Kind::XlV2 => "xl_v2",
        Kind::Mk2 => "mk2",
        Kind::Mk2Scissor => "mk2_scissor",
        Kind::MiniMk2 => "mini_mk2",
        Kind::MiniDiscord => "mini_discord",
        Kind::Neo => "neo",
        Kind::Pedal => "pedal",
        Kind::Plus => "plus",
        Kind::PlusXl => "plus_xl",
        Kind::MiniMk2Module => "mini_mk2_module",
        Kind::Mk2Module => "mk2_module",
        Kind::XlV2Module => "xl_v2_module",
    }
    .into()
}

fn kind_label(kind: Kind) -> String {
    match kind {
        Kind::Original => "Stream Deck",
        Kind::OriginalV2 => "Stream Deck (V2)",
        Kind::Mini => "Stream Deck Mini",
        Kind::Xl => "Stream Deck XL",
        Kind::XlV2 => "Stream Deck XL (V2)",
        Kind::Mk2 => "Stream Deck MK.2",
        Kind::Mk2Scissor => "Stream Deck MK.2 (Scissor)",
        Kind::MiniMk2 => "Stream Deck Mini MK.2",
        Kind::MiniDiscord => "Stream Deck Mini (Discord)",
        Kind::Neo => "Stream Deck Neo",
        Kind::Pedal => "Stream Deck Pedal",
        Kind::Plus => "Stream Deck +",
        Kind::PlusXl => "Stream Deck + XL",
        Kind::MiniMk2Module => "Stream Deck Mini MK.2 Module",
        Kind::Mk2Module => "Stream Deck MK.2 Module",
        Kind::XlV2Module => "Stream Deck XL Module",
    }
    .into()
}

/// Which dial a touch-strip coordinate belongs to.
///
/// The strip is split into one zone per dial (the zones sit above the dials on
/// a Stream Deck +). Devices without encoders fall back to their touch point
/// count, and anything unknown maps to dial 0 so a tap is never dropped.
fn touch_strip_index(kind: Kind, x: u16) -> usize {
    let zones = match kind.encoder_count() {
        0 => usize::from(kind.touchpoint_count()).max(1),
        encoders => usize::from(encoders),
    };
    let width = kind
        .lcd_strip_size()
        .map(|(width, _)| width)
        .filter(|width| *width > 0)
        .unwrap_or(800);
    (usize::from(x) * zones / width).min(zones - 1)
}

pub fn capabilities_from_kind(kind: Kind) -> SurfaceCapabilities {
    let (rows, cols) = kind.key_layout();
    let (w, h) = kind.key_image_format().size;
    let emulation_profile = match kind {
        Kind::Plus | Kind::PlusXl => EmulationProfile::StreamDeckPlus,
        Kind::Xl | Kind::XlV2 | Kind::XlV2Module => EmulationProfile::StreamDeckXl,
        Kind::Original | Kind::OriginalV2 => EmulationProfile::StreamDeckOriginal,
        _ => EmulationProfile::StreamDeckMk2,
    };
    SurfaceCapabilities {
        rows: rows as u32,
        columns: cols as u32,
        key_width_px: w as u32,
        key_height_px: h as u32,
        encoders: kind.encoder_count() as u32,
        has_lcd_strip: kind.lcd_strip_size().is_some(),
        emulation_profile,
    }
}

pub fn key_to_address(kind: Kind, key: u8) -> CellAddress {
    let columns = kind.column_count() as u32;
    CellAddress {
        row: (key as u32) / columns,
        column: (key as u32) % columns,
    }
}

pub fn address_to_key(kind: Kind, address: &CellAddress) -> u8 {
    (address.row * kind.column_count() as u32 + address.column) as u8
}

/// Keeps HidApi alive for the lifetime of a connected device.
pub struct HidSession {
    pub _hid: hidapi::HidApi,
    pub device: AsyncStreamDeck,
    pub kind: Kind,
}

impl HidSession {
    pub fn open(kind: Kind, serial: &str) -> Result<Self, String> {
        let mut hid = new_hidapi().map_err(|e| e.to_string())?;
        refresh_device_list(&mut hid).map_err(|e| e.to_string())?;
        let device = AsyncStreamDeck::connect(&hid, kind, serial).map_err(|e| e.to_string())?;
        Ok(Self {
            _hid: hid,
            device,
            kind,
        })
    }
}

/// Physical Stream Deck surface backed by HID.
pub struct StreamDeckHidSurface {
    descriptor: SurfaceDescriptor,
    session: HidSession,
    input_tx: broadcast::Sender<SurfaceInput>,
    touch_strip_ready: std::sync::atomic::AtomicBool,
}

impl StreamDeckHidSurface {
    pub fn new(
        id: SurfaceId,
        session: HidSession,
        label: impl Into<String>,
        device_id: impl Into<String>,
    ) -> Arc<Self> {
        let kind = session.kind;
        let caps = capabilities_from_kind(kind);
        Arc::new(Self {
            descriptor: SurfaceDescriptor {
                id,
                name: label.into(),
                capabilities: caps,
                backend: SurfaceBackend::Physical {
                    driver_id: crate::driver::DRIVER_ID.into(),
                    device_id: device_id.into(),
                },
            },
            session,
            input_tx: broadcast::channel(64).0,
            touch_strip_ready: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn kind(&self) -> Kind {
        self.session.kind
    }

    fn device(&self) -> &AsyncStreamDeck {
        &self.session.device
    }

    pub fn id(&self) -> SurfaceId {
        self.descriptor.id
    }

    pub fn input_sender(&self) -> broadcast::Sender<SurfaceInput> {
        self.input_tx.clone()
    }

    pub fn subscribe_inputs(&self) -> broadcast::Receiver<SurfaceInput> {
        self.input_tx.subscribe()
    }

    pub fn emit_input(&self, input: SurfaceInput) {
        let _ = self.input_tx.send(input);
    }

    pub async fn firmware_version(&self) -> Result<String, String> {
        self.device()
            .firmware_version()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn set_brightness(&self, percent: u8) -> Result<(), String> {
        self.device()
            .set_brightness(percent.min(100))
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn run_input_loop(surface: Arc<Self>, poll_rate: f32) {
        let reader = surface.device().get_reader();
        loop {
            match reader.read(poll_rate).await {
                Ok(updates) => {
                    for update in updates {
                        let kind = surface.kind();
                        let input = match update {
                            DeviceStateUpdate::ButtonDown(key) => Some(SurfaceInput::KeyDown {
                                address: key_to_address(kind, key),
                            }),
                            DeviceStateUpdate::ButtonUp(key) => Some(SurfaceInput::KeyUp {
                                address: key_to_address(kind, key),
                            }),
                            DeviceStateUpdate::EncoderTwist(index, ticks) => {
                                Some(SurfaceInput::EncoderRotate {
                                    index: index as u32,
                                    ticks: ticks as i32,
                                    pressed: false,
                                })
                            }
                            DeviceStateUpdate::EncoderDown(index) => {
                                Some(SurfaceInput::EncoderPress {
                                    index: index as u32,
                                    pressed: true,
                                })
                            }
                            DeviceStateUpdate::EncoderUp(index) => {
                                Some(SurfaceInput::EncoderPress {
                                    index: index as u32,
                                    pressed: false,
                                })
                            }
                            DeviceStateUpdate::TouchPointDown(index) => {
                                Some(SurfaceInput::EncoderPress {
                                    index: index as u32,
                                    pressed: true,
                                })
                            }
                            DeviceStateUpdate::TouchPointUp(index) => {
                                Some(SurfaceInput::EncoderPress {
                                    index: index as u32,
                                    pressed: false,
                                })
                            }
                            DeviceStateUpdate::TouchScreenPress(x, y) => {
                                Some(SurfaceInput::TouchTap {
                                    index: touch_strip_index(kind, x) as u32,
                                    x: u32::from(x),
                                    y: u32::from(y),
                                    hold: false,
                                })
                            }
                            DeviceStateUpdate::TouchScreenLongPress(x, y) => {
                                Some(SurfaceInput::TouchTap {
                                    index: touch_strip_index(kind, x) as u32,
                                    x: u32::from(x),
                                    y: u32::from(y),
                                    hold: true,
                                })
                            }
                            _ => None,
                        };
                        if let Some(input) = input {
                            surface.emit_input(input);
                        }
                    }
                }
                Err(e) => {
                    warn!("HID input loop ended: {e}");
                    break;
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl super::Surface for StreamDeckHidSurface {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        let caps = &self.descriptor.capabilities;
        let (width, height) = (caps.key_width_px, caps.key_height_px);

        if caps.has_lcd_strip
            && !self
                .touch_strip_ready
                .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            // First paint: blank the strip so stale firmware content is gone.
            // Dial feedback replaces it through `render_touch_strip`.
            if let Some((w, h)) = self.kind().lcd_strip_size() {
                let image = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                    w as u32,
                    h as u32,
                    image::Rgba([16, 18, 24, 255]),
                ));
                if let Some(format) = self.kind().lcd_image_format() {
                    if let Ok(data) = convert_image_with_format(format, image) {
                        let _ = self.session.device.write_lcd_fill(&data).await;
                    }
                }
            }
        }

        for update in updates {
            let key = address_to_key(self.kind(), &update.address);
            let has_title = update
                .visual
                .title
                .as_ref()
                .is_some_and(|t| !t.trim().is_empty());

            let hints = KeyDisplayHints::new(width, height);

            let decoded = if has_title {
                ideck_core::compose_key_png(&update.visual, hints).and_then(|png| {
                    ImageReader::new(Cursor::new(png))
                        .with_guessed_format()
                        .ok()?
                        .decode()
                        .ok()
                })
            } else if let Some(image) = &update.visual.image {
                let Ok(reader) = ImageReader::new(Cursor::new(&image.data)).with_guessed_format()
                else {
                    continue;
                };
                let Ok(img) = reader.decode() else {
                    continue;
                };
                Some(img)
            } else {
                None
            };

            let img = if let Some(img) = decoded {
                img
            } else {
                let blank = ideck_core::solid_key_png(width, height, 0, 0, 0);
                match ImageReader::new(Cursor::new(&blank.data))
                    .with_guessed_format()
                    .ok()
                    .and_then(|r| r.decode().ok())
                {
                    Some(img) => img,
                    None => {
                        warn!("blank key image decode failed");
                        continue;
                    }
                }
            };

            let converted = if has_title {
                convert_key_image_with_quality(self.kind(), img, 98)
            } else {
                convert_key_image_with_quality(self.kind(), img, 90)
            }
            .map_err(|e| SurfaceError::Other(e.to_string()))?;
            self.device()
                .write_image(key, &converted)
                .await
                .map_err(|e| SurfaceError::Other(e.to_string()))?;
        }

        self.device()
            .flush()
            .await
            .map_err(|e| SurfaceError::Other(e.to_string()))?;
        Ok(())
    }

    async fn render_touch_strip(
        &self,
        segments: &[ideck_core::TouchStripSegment],
    ) -> Result<(), SurfaceError> {
        let kind = self.kind();
        tracing::debug!(?kind, segments = segments.len(), "touch strip requested");
        let Some((w, h)) = kind.lcd_strip_size() else {
            tracing::debug!(?kind, "touch strip: kind has no strip");
            return Ok(());
        };
        let zones = u32::from(kind.encoder_count()).max(1);
        let Some(payload) = ideck_core::render_touch_strip(w as u32, h as u32, segments, zones)
        else {
            return Ok(());
        };
        let Ok(image) = ImageReader::new(Cursor::new(&payload.data))
            .with_guessed_format()
            .and_then(|reader| reader.decode().map_err(std::io::Error::other))
        else {
            return Ok(());
        };
        let Some(format) = kind.lcd_image_format() else {
            return Ok(());
        };
        let data = convert_image_with_format(format, image)
            .map_err(|e| SurfaceError::Other(e.to_string()))?;
        self.device()
            .write_lcd_fill(&data)
            .await
            .map_err(|e| SurfaceError::Other(e.to_string()))?;
        self.touch_strip_ready
            .store(true, std::sync::atomic::Ordering::SeqCst);
        tracing::debug!(
            width = w,
            height = h,
            zones,
            segments = segments.len(),
            bytes = data.len(),
            "touch strip rendered"
        );
        Ok(())
    }
}

/// Device key upload: same transforms as `elgato_streamdeck`, with configurable JPEG quality.
fn convert_key_image_with_quality(
    kind: Kind,
    image: DynamicImage,
    jpeg_quality: u8,
) -> Result<Vec<u8>, image::ImageError> {
    let format = kind.key_image_format();
    match format.mode {
        elgato_streamdeck::info::ImageMode::JPEG => {}
        _ => return convert_image_with_format(format, image),
    }

    let (ws, hs) = format.size;
    let image = match format.rotation {
        elgato_streamdeck::info::ImageRotation::Rot0 => image,
        elgato_streamdeck::info::ImageRotation::Rot90 => image.rotate90(),
        elgato_streamdeck::info::ImageRotation::Rot180 => image.rotate180(),
        elgato_streamdeck::info::ImageRotation::Rot270 => image.rotate270(),
    };
    let image = image.resize_exact(ws as u32, hs as u32, FilterType::Triangle);
    let image = match format.mirror {
        elgato_streamdeck::info::ImageMirroring::None => image,
        elgato_streamdeck::info::ImageMirroring::X => image.fliph(),
        elgato_streamdeck::info::ImageMirroring::Y => image.flipv(),
        elgato_streamdeck::info::ImageMirroring::Both => image.fliph().flipv(),
    };
    let rgb = image.into_rgb8();
    let mut buf = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, jpeg_quality);
    encoder.encode(rgb.as_raw(), ws as u32, hs as u32, ColorType::Rgb8.into())?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_touch_strip_maps_to_the_dial_under_the_finger() {
        // 4 dials across an 800px strip: 200px per zone.
        assert_eq!(touch_strip_index(Kind::Plus, 0), 0);
        assert_eq!(touch_strip_index(Kind::Plus, 199), 0);
        assert_eq!(touch_strip_index(Kind::Plus, 200), 1);
        assert_eq!(touch_strip_index(Kind::Plus, 450), 2);
        assert_eq!(touch_strip_index(Kind::Plus, 799), 3);
    }

    #[test]
    fn touch_strip_index_tolerates_devices_without_encoders() {
        // Neo has touch points but no dials; a tap still resolves to a zone.
        assert_eq!(touch_strip_index(Kind::Neo, 0), 0);
        assert!(touch_strip_index(Kind::Neo, 200) < 2);
    }
}
