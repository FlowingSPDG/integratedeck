//! Stream Deck Studio over the legacy LAN transport (TCP 5343).
//!
//! The device sends a 512-byte keep-alive, accepts 1024-byte command packets,
//! and key images use `02 07 <key> <last> <size16> <chunk16> <jpeg>`.

use std::sync::Arc;

use async_trait::async_trait;
use ideck_core::{SurfaceId, VisualState};
use crate::EmulationProfile;
use image::RgbaImage;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};

use super::{DiscoveredDevice, PhysicalSurface, SurfaceDriverManifest, SurfaceDriverPlugin};
use crate::{
    CellAddress, CellUpdate, Surface, SurfaceCapabilities, SurfaceDescriptor, SurfaceError,
    SurfaceInput,
};

pub const STUDIO_DRIVER_ID: &str = "studio-lan";
const KEY_W: u32 = 144;
const KEY_H: u32 = 112;
const ROWS: u32 = 2;
const COLS: u32 = 16;
const ENCODERS: u32 = 2;

pub struct StudioLanDriver;

impl SurfaceDriverPlugin for StudioLanDriver {
    fn manifest(&self) -> SurfaceDriverManifest {
        SurfaceDriverManifest {
            id: STUDIO_DRIVER_ID.into(),
            name: "Stream Deck Studio (LAN)".into(),
            description: "Studio over the legacy TCP 5343 protocol (PoE).".into(),
        }
    }

    fn scan(&self) -> Result<Vec<DiscoveredDevice>, SurfaceError> {
        let addr = studio_addr();
        Ok(vec![DiscoveredDevice {
            driver_id: STUDIO_DRIVER_ID.into(),
            device_id: addr.clone(),
            label: format!("Stream Deck Studio ({addr})"),
            product: "Stream Deck Studio (LAN)".into(),
            rows: ROWS,
            columns: COLS,
        }])
    }

    fn connect(
        &self,
        device_id: &str,
        surface_id: SurfaceId,
        label: &str,
    ) -> Result<Arc<dyn PhysicalSurface>, SurfaceError> {
        Ok(StudioLanSurface::start(surface_id, label, device_id))
    }
}

pub fn studio_addr() -> String {
    std::env::var("IDECK_STUDIO_ADDR").unwrap_or_else(|_| "192.168.3.48:5343".into())
}

struct StudioLanSurface {
    id: SurfaceId,
    descriptor: SurfaceDescriptor,
    commands: mpsc::UnboundedSender<Vec<u8>>,
    inputs: broadcast::Sender<SurfaceInput>,
}

impl StudioLanSurface {
    fn start(id: SurfaceId, label: &str, addr: &str) -> Arc<Self> {
        let (commands, rx) = mpsc::unbounded_channel();
        let (inputs, _) = broadcast::channel(64);
        let surface = Arc::new(Self {
            id,
            descriptor: SurfaceDescriptor {
                id,
                name: label.into(),
                capabilities: SurfaceCapabilities {
                    rows: ROWS,
                    columns: COLS,
                    key_width_px: KEY_W,
                    key_height_px: KEY_H,
                    encoders: ENCODERS,
                    has_lcd_strip: true,
                    emulation_profile: EmulationProfile::Custom,
                },
                backend: crate::SurfaceBackend::Physical {
                    driver_id: STUDIO_DRIVER_ID.into(),
                    device_id: addr.into(),
                },
            },
            commands,
            inputs,
        });

        let addr = addr.to_string();
        let inputs_tx = surface.inputs.clone();
        tokio::spawn(async move {
            if let Err(error) = network_task(&addr, rx, inputs_tx).await {
                warn!("studio LAN {addr}: {error:#}");
            }
        });
        surface
    }

    fn send(&self, packet: Vec<u8>) {
        let _ = self.commands.send(packet);
    }

    fn send_image(&self, key: u8, jpeg: &[u8]) {        const CHUNK: usize = 1016;
        let mut offset = 0;
        let mut sequence = 0u16;
        while offset < jpeg.len() {
            let end = (offset + CHUNK).min(jpeg.len());
            let chunk_len = (end - offset) as u16;
            let mut packet = vec![0u8; 1024];
            packet[0] = 0x02;
            packet[1] = 0x07;
            packet[2] = key;
            packet[3] = u8::from(end == jpeg.len());
            packet[4..6].copy_from_slice(&chunk_len.to_le_bytes());
            packet[6..8].copy_from_slice(&sequence.to_le_bytes());
            packet[8..8 + (end - offset)].copy_from_slice(&jpeg[offset..end]);
            self.send(packet);
            sequence = sequence.wrapping_add(1);
            offset = end;
        }
    }

    async fn render_inner(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        for update in updates {
            let key = (update.address.row * COLS + update.address.column) as u8;
            if let Some(color) = update.visual.bgcolor {
                let mut packet = vec![0u8; 1024];
                packet[0] = 0x03;
                packet[1] = 0x06;
                packet[2] = key;
                packet[3] = color.r;
                packet[4] = color.g;
                packet[5] = color.b;
                self.send(packet);
            }
            if let Some(image) = &update.visual.image {
                if let Some(jpeg) = encode_key_image(image, &update.visual) {
                    self.send_image(key, &jpeg);
                }
            }
        }
        Ok(())
    }
}

async fn network_task(
    addr: &str,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
    inputs: broadcast::Sender<SurfaceInput>,
) -> anyhow::Result<()> {
    let stream = TcpStream::connect(addr).await?;
    stream.set_nodelay(true).ok();
    info!("studio LAN connected {addr}");
    let (mut read_half, mut write_half) = stream.into_split();

    let mut keepalive = [0u8; 512];
    read_half.read_exact(&mut keepalive).await?;

    let mut ack = [0u8; 1024];
    ack[0] = 0x03;
    ack[1] = 0x1a;
    write_half.write_all(&ack).await?;

    let mut brightness = [0u8; 1024];
    brightness[0] = 0x03;
    brightness[1] = 0x08;
    brightness[2] = 64;
    write_half.write_all(&brightness).await?;

    tokio::spawn(async move {
        while let Some(packet) = rx.recv().await {
            if write_half.write_all(&packet).await.is_err() {
                break;
            }
        }
    });

    let mut report = [0u8; 512];
    let mut previous = [false; 32];
    loop {
        if read_half.read_exact(&mut report).await.is_err() {
            break;
        }
        // Gen2 button report: 01 00 <len16> <0/1 per key>.
        if report[0] == 0x01 && report[1] == 0x00 {
            let len = u16::from_le_bytes([report[2], report[3]]) as usize;
            for index in 0..len.min(32) {
                let pressed = report.get(4 + index).copied().unwrap_or(0) != 0;
                if pressed == previous[index] {
                    continue;
                }
                previous[index] = pressed;
                let address = CellAddress {
                    row: (index / COLS as usize) as u32,
                    column: (index % COLS as usize) as u32,
                };
                let input = if pressed {
                    SurfaceInput::KeyDown { address }
                } else {
                    SurfaceInput::KeyUp { address }
                };
                let _ = inputs.send(input);
            }
        }
    }
    Ok(())
}

#[async_trait]
impl PhysicalSurface for StudioLanSurface {
    fn surface_id(&self) -> SurfaceId {
        self.id
    }

    fn subscribe_inputs(&self) -> broadcast::Receiver<SurfaceInput> {
        self.inputs.subscribe()
    }

    fn spawn_input_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {})
    }

    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    fn capabilities(&self) -> &SurfaceCapabilities {
        &self.descriptor.capabilities
    }

    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        self.render_inner(updates).await
    }

    fn as_any(self: Arc<Self>) -> Arc<dyn std::any::Any + Send + Sync> {
        self
    }
}

fn encode_key_image(
    image: &ideck_core::ImagePayload,
    visual: &VisualState,
) -> Option<Vec<u8>> {
    let decoded = image::load_from_memory(&image.data).ok()?;
    let mut canvas = RgbaImage::new(KEY_W, KEY_H);
    if let Some(color) = visual.bgcolor {
        for pixel in canvas.pixels_mut() {
            *pixel = image::Rgba([color.r, color.g, color.b, 255]);
        }
    }
    let resized = decoded.resize_to_fill(KEY_W, KEY_H, image::imageops::FilterType::Triangle);
    image::imageops::overlay(&mut canvas, &resized.to_rgba8(), 0, 0);
    let mut jpeg = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90);
    encoder
        .encode_image(&image::DynamicImage::ImageRgba8(canvas))
        .ok()?;
    Some(jpeg)
}

#[async_trait]
impl Surface for StudioLanSurface {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        self.render_inner(updates).await
    }
}


