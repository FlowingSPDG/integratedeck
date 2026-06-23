use std::sync::Arc;

use async_trait::async_trait;
use ideck_core::SurfaceId;

use super::{
    DiscoveredDevice, PhysicalSurface, SurfaceDriverManifest, SurfaceDriverPlugin,
};
use crate::hid::{
    self, HidSession, StreamDeckHidSurface, capabilities_from_kind, parse_kind_id,
};
use crate::{CellUpdate, Surface, SurfaceCapabilities, SurfaceDescriptor, SurfaceError, SurfaceInput};

pub const DRIVER_ID: &str = "streamdeck-hid";

/// Stream Deck family via USB HID (`elgato-streamdeck`).
pub struct StreamDeckHidDriver;

impl SurfaceDriverPlugin for StreamDeckHidDriver {
    fn manifest(&self) -> SurfaceDriverManifest {
        SurfaceDriverManifest {
            id: DRIVER_ID.into(),
            name: "Elgato Stream Deck (USB HID)".into(),
            description: "Official Stream Deck devices over USB HID.".into(),
        }
    }

    fn scan(&self) -> Result<Vec<DiscoveredDevice>, SurfaceError> {
        hid::scan_hid_devices()
            .map_err(SurfaceError::Other)
            .map(|devices| {
                devices
                    .into_iter()
                    .map(|d| DiscoveredDevice {
                        driver_id: DRIVER_ID.into(),
                        device_id: format!("{}:{}", d.kind, d.serial),
                        label: format!("{} ({})", d.product, d.serial),
                        product: d.product,
                        rows: d.rows,
                        columns: d.columns,
                    })
                    .collect()
            })
    }

    fn connect(
        &self,
        device_id: &str,
        surface_id: SurfaceId,
        label: &str,
    ) -> Result<Arc<dyn PhysicalSurface>, SurfaceError> {
        let (kind_id, serial) = device_id
            .split_once(':')
            .ok_or_else(|| SurfaceError::Other(format!("invalid device_id: {device_id}")))?;
        let kind = parse_kind_id(kind_id)
            .ok_or_else(|| SurfaceError::Other(format!("unknown Stream Deck kind: {kind_id}")))?;
        let session = HidSession::open(kind, serial).map_err(SurfaceError::Other)?;
        let surface = StreamDeckHidSurface::new(surface_id, session, label, device_id);
        Ok(surface)
    }
}

#[async_trait]
impl PhysicalSurface for StreamDeckHidSurface {
    fn surface_id(&self) -> SurfaceId {
        self.id()
    }

    fn subscribe_inputs(&self) -> tokio::sync::broadcast::Receiver<SurfaceInput> {
        StreamDeckHidSurface::subscribe_inputs(self)
    }

    fn spawn_input_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            // 1000 Hz polling: ~1 ms worst-case detection latency (30 Hz was ~33 ms).
            StreamDeckHidSurface::run_input_loop(self, 1000.0).await;
        })
    }

    fn descriptor(&self) -> &SurfaceDescriptor {
        Surface::descriptor(self)
    }

    fn capabilities(&self) -> &SurfaceCapabilities {
        &Surface::descriptor(self).capabilities
    }

    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        Surface::render(self, updates).await
    }

    fn as_any(self: Arc<Self>) -> Arc<dyn std::any::Any + Send + Sync> {
        self
    }
}

pub fn parse_streamdeck_device_id(device_id: &str) -> Option<(String, String)> {
    device_id.split_once(':').map(|(k, s)| (k.to_string(), s.to_string()))
}

pub fn capabilities_for_kind_id(kind_id: &str) -> Option<SurfaceCapabilities> {
    parse_kind_id(kind_id).map(capabilities_from_kind)
}
