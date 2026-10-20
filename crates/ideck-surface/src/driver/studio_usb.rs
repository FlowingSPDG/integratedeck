//! Stream Deck Studio over USB HID (`0fd9:00aa`).
//!
//! `elgato-streamdeck` does not know this PID, so the driver talks to
//! `hidapi` directly: 1024-byte output reports, 512-byte input reports and
//! 32-byte feature reports. The wire commands are shared with the LAN driver
//! through [`super::studio_protocol`].

use std::sync::mpsc::{self, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use hidapi::{HidApi, HidDevice};
use ideck_core::SurfaceId;
use tracing::{info, warn};

use super::studio_protocol::{
    decode_input, encode_key_jpeg, key_color_packet, key_image_packets, KEY_HEIGHT, KEY_WIDTH,
    KEYS, ENCODERS, INPUT_REPORT_LEN, ROWS, COLS,
};
use super::{DiscoveredDevice, PhysicalSurface, SurfaceDriverManifest, SurfaceDriverPlugin};
use crate::{
    CellUpdate, EmulationProfile, Surface, SurfaceBackend, SurfaceCapabilities, SurfaceDescriptor,
    SurfaceError, SurfaceInput,
};

pub const STUDIO_USB_DRIVER_ID: &str = "studio-usb";
const VENDOR_ID: u16 = 0x0fd9;
const PRODUCT_ID: u16 = 0x00aa;
const CONSUMER_USAGE_PAGE: u16 = 0x0c;

pub struct StudioUsbDriver;

impl SurfaceDriverPlugin for StudioUsbDriver {
    fn manifest(&self) -> SurfaceDriverManifest {
        SurfaceDriverManifest {
            id: STUDIO_USB_DRIVER_ID.into(),
            name: "Stream Deck Studio (USB)".into(),
            description: "Studio over USB HID (0fd9:00aa).".into(),
        }
    }

    fn scan(&self) -> Result<Vec<DiscoveredDevice>, SurfaceError> {
        Ok(scan_devices()?
            .into_iter()
            .map(|device| DiscoveredDevice {
                driver_id: STUDIO_USB_DRIVER_ID.into(),
                device_id: device.device_id.clone(),
                label: format!("Stream Deck Studio USB ({})", device.device_id),
                product: "Stream Deck Studio (USB)".into(),
                rows: ROWS,
                columns: COLS,
            })
            .collect())
    }

    fn connect(
        &self,
        device_id: &str,
        surface_id: SurfaceId,
        label: &str,
    ) -> Result<Arc<dyn PhysicalSurface>, SurfaceError> {
        StudioUsbSurface::start(surface_id, label, device_id)
            .map(|surface| surface as Arc<dyn PhysicalSurface>)
    }
}

struct UsbScanEntry {
    device_id: String,
}

fn scan_devices() -> Result<Vec<UsbScanEntry>, SurfaceError> {
    let api = HidApi::new().map_err(|error| SurfaceError::Other(error.to_string()))?;
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for info in api.device_list() {
        if info.vendor_id() != VENDOR_ID || info.product_id() != PRODUCT_ID {
            continue;
        }
        if info.usage_page() != 0 && info.usage_page() != CONSUMER_USAGE_PAGE {
            continue;
        }
        let serial = info.serial_number().unwrap_or_default().to_string();
        let device_id = if serial.is_empty() {
            info.path().to_string_lossy().to_string()
        } else {
            serial
        };
        if seen.insert(device_id.clone()) {
            entries.push(UsbScanEntry { device_id });
        }
    }
    Ok(entries)
}

fn open_device(device_id: &str) -> Result<HidDevice, SurfaceError> {
    let api = HidApi::new().map_err(|error| SurfaceError::Other(error.to_string()))?;
    let mut candidate = None;
    for info in api.device_list() {
        if info.vendor_id() != VENDOR_ID || info.product_id() != PRODUCT_ID {
            continue;
        }
        let serial = info.serial_number().unwrap_or_default().to_string();
        let path = info.path().to_string_lossy().to_string();
        if serial == device_id || path == device_id {
            candidate = Some(info);
            break;
        }
    }
    let info = candidate.ok_or_else(|| {
        SurfaceError::Other(format!("Stream Deck Studio USB not found: {device_id}"))
    })?;
    info.open_device(&api)
        .map_err(|error| SurfaceError::Other(error.to_string()))
}

pub struct StudioUsbSurface {
    id: SurfaceId,
    descriptor: SurfaceDescriptor,
    commands: mpsc::Sender<Vec<u8>>,
    inputs: tokio::sync::broadcast::Sender<SurfaceInput>,
}

impl StudioUsbSurface {
    fn start(id: SurfaceId, label: &str, device_id: &str) -> Result<Arc<Self>, SurfaceError> {
        // Validate that the device is present now; the worker re-opens it after
        // unplug/replug or driver errors.
        let _ = open_device(device_id)?;
        let (commands, rx) = mpsc::channel::<Vec<u8>>();
        let (inputs, _) = tokio::sync::broadcast::channel(64);
        let descriptor = SurfaceDescriptor {
            id,
            name: label.into(),
            capabilities: SurfaceCapabilities {
                rows: ROWS,
                columns: COLS,
                key_width_px: KEY_WIDTH,
                key_height_px: KEY_HEIGHT,
                encoders: ENCODERS as u32,
                has_lcd_strip: true,
                emulation_profile: EmulationProfile::Custom,
            },
            backend: SurfaceBackend::Physical {
                driver_id: STUDIO_USB_DRIVER_ID.into(),
                device_id: device_id.into(),
            },
        };
        let surface = Arc::new(Self {
            id,
            descriptor,
            commands,
            inputs: inputs.clone(),
        });
        let worker_device_id = device_id.to_string();
        std::thread::Builder::new()
            .name("studio-usb".into())
            .spawn(move || usb_task(worker_device_id, rx, inputs))
            .map_err(|error| SurfaceError::Other(error.to_string()))?;
        info!("studio USB opened {device_id}");
        Ok(surface)
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
        self.send(super::studio_protocol::encoder_led_packet(
            encoder,
            (r, g, b),
        ));
    }

    /// Draw a JPEG on the Studio fullscreen LCD panel.
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

fn reconnect(device_id: &str) -> HidDevice {
    loop {
        match open_device(device_id) {
            Ok(device) => {
                let _ = device.send_feature_report(&[0x03, 0x08, 64]);
                return device;
            }
            Err(error) => {
                warn!("studio USB {device_id}: reconnect failed: {error}; retrying");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

fn usb_task(
    device_id: String,
    rx: mpsc::Receiver<Vec<u8>>,
    inputs: tokio::sync::broadcast::Sender<SurfaceInput>,
) {
    let mut device = reconnect(&device_id);
    let mut previous_keys = [false; KEYS];
    let mut previous_encoders = [false; ENCODERS];
    let mut report = [0u8; INPUT_REPORT_LEN];
    let mut logged = 0usize;
    loop {
        match rx.try_recv() {
            Ok(packet) => {
                if let Err(error) = device.write(&packet) {
                    warn!("studio USB write failed: {error}; reconnecting");
                    device = reconnect(&device_id);
                    continue;
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => return,
        }
        match device.read_timeout(&mut report, 20) {
            Ok(0) => {}
            Ok(length) => {
                if logged < 24 {
                    info!("studio USB rx head={:02x?}", &report[..16]);
                    logged += 1;
                }
                for input in decode_input(
                    &report[..length.min(report.len())],
                    &mut previous_keys,
                    &mut previous_encoders,
                ) {
                    let _ = inputs.send(input);
                }
            }
            Err(error) => {
                warn!("studio USB read failed: {error}; reconnecting");
                device = reconnect(&device_id);
            }
        }
    }
}

#[async_trait]
impl PhysicalSurface for StudioUsbSurface {
    fn surface_id(&self) -> SurfaceId {
        self.id
    }

    fn subscribe_inputs(&self) -> tokio::sync::broadcast::Receiver<SurfaceInput> {
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
impl Surface for StudioUsbSurface {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        self.render_inner(updates).await
    }
}
