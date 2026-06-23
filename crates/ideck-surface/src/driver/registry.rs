use std::sync::Arc;

use ideck_core::SurfaceId;

use super::{DiscoveredDevice, PhysicalSurface, SurfaceDriverManifest, SurfaceDriverPlugin};
use crate::SurfaceError;

/// Registry of installed surface driver plugins.
pub struct SurfaceDriverRegistry {
    drivers: Vec<Arc<dyn SurfaceDriverPlugin>>,
}

impl Default for SurfaceDriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceDriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: vec![Arc::new(super::StreamDeckHidDriver)],
        }
    }

    pub fn list_manifests(&self) -> Vec<SurfaceDriverManifest> {
        self.drivers.iter().map(|d| d.manifest()).collect()
    }

    pub fn scan_all(&self) -> Result<Vec<DiscoveredDevice>, SurfaceError> {
        let mut devices = Vec::new();
        for driver in &self.drivers {
            devices.extend(driver.scan()?);
        }
        devices.sort_by(|a, b| a.product.cmp(&b.product).then(a.label.cmp(&b.label)));
        Ok(devices)
    }

    pub fn connect(
        &self,
        driver_id: &str,
        device_id: &str,
        surface_id: SurfaceId,
        label: &str,
    ) -> Result<Arc<dyn PhysicalSurface>, SurfaceError> {
        let driver = self
            .drivers
            .iter()
            .find(|d| d.manifest().id == driver_id)
            .ok_or_else(|| SurfaceError::Other(format!("unknown surface driver: {driver_id}")))?;
        driver.connect(device_id, surface_id, label)
    }

    pub fn driver(&self, driver_id: &str) -> Option<&Arc<dyn SurfaceDriverPlugin>> {
        self.drivers.iter().find(|d| d.manifest().id == driver_id)
    }
}
