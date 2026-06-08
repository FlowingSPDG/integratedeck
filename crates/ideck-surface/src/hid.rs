use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use elgato_streamdeck::asynchronous::AsyncStreamDeck;
use elgato_streamdeck::images::convert_image_async;
use elgato_streamdeck::info::Kind;
use elgato_streamdeck::{list_devices, new_hidapi, refresh_device_list, DeviceStateUpdate};
use image::ImageReader;
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
    device: AsyncStreamDeck,
    kind: Kind,
    input_tx: broadcast::Sender<SurfaceInput>,
}

impl StreamDeckHidSurface {
    pub fn new(id: SurfaceId, session: &HidSession, label: impl Into<String>) -> Arc<Self> {
        let caps = capabilities_from_kind(session.kind);
        Arc::new(Self {
            descriptor: SurfaceDescriptor {
                id,
                name: label.into(),
                capabilities: caps,
                backend: SurfaceBackend::StreamDeckHid,
            },
            device: session.device.clone(),
            kind: session.kind,
            input_tx: broadcast::channel(64).0,
        })
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

    pub async fn run_input_loop(surface: Arc<Self>, poll_rate: f32) {
        let reader = surface.device.get_reader();
        loop {
            match reader.read(poll_rate).await {
                Ok(updates) => {
                    for update in updates {
                        let input = match update {
                            DeviceStateUpdate::ButtonDown(key) => Some(SurfaceInput::KeyDown {
                                address: key_to_address(surface.kind, key),
                            }),
                            DeviceStateUpdate::ButtonUp(key) => Some(SurfaceInput::KeyUp {
                                address: key_to_address(surface.kind, key),
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

#[async_trait]
impl super::Surface for StreamDeckHidSurface {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        for update in updates {
            let key = address_to_key(self.kind, &update.address);
            if let Some(image) = &update.visual.image {
                let Ok(reader) =
                    ImageReader::new(Cursor::new(&image.data)).with_guessed_format()
                else {
                    continue;
                };
                let Ok(img) = reader.decode() else {
                    continue;
                };
                let converted = convert_image_async(self.kind, img)
                    .map_err(|e| SurfaceError::Other(e.to_string()))?;
                self.device
                    .write_image(key, &converted)
                    .await
                    .map_err(|e| SurfaceError::Other(e.to_string()))?;
            }
        }
        self.device
            .flush()
            .await
            .map_err(|e| SurfaceError::Other(e.to_string()))?;
        Ok(())
    }
}
