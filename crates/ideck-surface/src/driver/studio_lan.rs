//! Stream Deck Studio over the legacy LAN transport (TCP 5343).
//!
//! The device sends a 512-byte keep-alive, accepts 1024-byte command packets,
//! and key images use `02 07 <key> <last> <size16> <chunk16> <jpeg>`.

use std::sync::Arc;

use crate::EmulationProfile;
use async_trait::async_trait;
use ideck_core::SurfaceId;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};

use super::studio_protocol::{
    brightness_packet, decode_input, encode_key_jpeg, encoder_led_packet, key_color_packet,
    key_image_packets, ENCODERS as PROTOCOL_ENCODERS, KEYS,
};
use super::{DiscoveredDevice, PhysicalSurface, SurfaceDriverManifest, SurfaceDriverPlugin};
use crate::{
    CellUpdate, Surface, SurfaceCapabilities, SurfaceDescriptor, SurfaceError, SurfaceInput,
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

pub struct StudioLanSurface {
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
            network_task(addr, rx, inputs_tx).await;
        });
        surface
    }

    fn send(&self, packet: Vec<u8>) {
        let _ = self.commands.send(packet);
    }

    fn send_image(&self, key: u8, jpeg: &[u8]) {
        for packet in key_image_packets(key, jpeg) {
            self.send(packet);
        }
    }

    /// Set all LEDs of one encoder ring to an RGB colour.
    pub fn set_encoder_led(&self, encoder: u8, r: u8, g: u8, b: u8) {
        self.send(encoder_led_packet(encoder, (r, g, b)));
    }

    /// Draw a JPEG on the Studio fullscreen LCD panel.
    /// Set panel brightness (0-100).
    pub fn set_brightness(&self, percent: u8) {
        self.send(super::studio_protocol::brightness_packet(percent.min(100)));
    }

    pub fn set_lcd_image(&self, jpeg: &[u8]) {
        for packet in super::studio_protocol::lcd_image_packets(0, 0, 1424, 280, jpeg) {
            self.send(packet);
        }
    }

    async fn render_inner(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        for update in updates {
            let key = (update.address.row * COLS + update.address.column) as u8;
            if let Some(color) = update.visual.bgcolor {
                self.send(key_color_packet(key, (color.r, color.g, color.b)));
            }
            if let Some(image) = &update.visual.image {
                if let Some(jpeg) = encode_key_jpeg(image, &update.visual) {
                    self.send_image(key, &jpeg);
                }
            }
        }
        Ok(())
    }
}

/// Keeps the LAN transport alive: reconnect with a 5s backoff after any error.
async fn network_task(
    addr: String,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
    inputs: broadcast::Sender<SurfaceInput>,
) {
    loop {
        match TcpStream::connect(&addr).await {
            Ok(stream) => {
                stream.set_nodelay(true).ok();
                info!("studio LAN connected {addr}");
                if let Err(error) = run_session(stream, &mut rx, &inputs).await {
                    warn!("studio LAN {addr}: {error:#}; reconnecting");
                }
            }
            Err(error) => warn!("studio LAN {addr}: connect failed: {error}; retrying"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn run_session(
    stream: TcpStream,
    rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    inputs: &broadcast::Sender<SurfaceInput>,
) -> anyhow::Result<()> {
    let (mut read_half, mut write_half) = stream.into_split();

    let mut keepalive = [0u8; 512];
    read_half.read_exact(&mut keepalive).await?;

    let mut ack = [0u8; 1024];
    ack[0] = 0x03;
    ack[1] = 0x1a;
    write_half.write_all(&ack).await?;

    let brightness = brightness_packet(64);
    write_half.write_all(&brightness).await?;

    let mut report = [0u8; 512];
    let mut filled = 0usize;
    let mut previous = [false; KEYS];
    let mut previous_encoders = [false; PROTOCOL_ENCODERS];
    let mut logged = 0usize;
    loop {
        tokio::select! {
            command = rx.recv() => {
                let Some(packet) = command else {
                    return Ok(());
                };
                write_half.write_all(&packet).await?;
            }
            read = read_half.read(&mut report[filled..]) => {
                let length = read?;
                if length == 0 {
                    anyhow::bail!("connection closed");
                }
                filled += length;
                if filled == report.len() {
                    if logged < 24 {
                        info!("studio rx head={:02x?}", &report[..16]);
                        logged += 1;
                    }
                    for input in decode_input(&report, &mut previous, &mut previous_encoders) {
                        let _ = inputs.send(input);
                    }
                    filled = 0;
                }
            }
        }
    }
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

#[async_trait]
impl Surface for StudioLanSurface {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        self.render_inner(updates).await
    }
}
