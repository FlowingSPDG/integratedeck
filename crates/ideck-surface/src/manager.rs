use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{broadcast, RwLock};

use crate::driver::PhysicalSurface;
use crate::{CellUpdate, Surface, SurfaceCapabilities, SurfaceDescriptor, SurfaceError, SurfaceInput};
use ideck_core::SurfaceId;

/// Erases [`PhysicalSurface`] for storage as [`Surface`].
struct ErasedPhysicalSurface(Arc<dyn PhysicalSurface>);

#[async_trait]
impl Surface for ErasedPhysicalSurface {
    fn descriptor(&self) -> &SurfaceDescriptor {
        Surface::descriptor(self.0.as_ref())
    }

    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        Surface::render(self.0.as_ref(), updates).await
    }
}

/// Mock surface for development and tests.
pub struct MockSurface {
    descriptor: SurfaceDescriptor,
    input_tx: broadcast::Sender<SurfaceInput>,
    cell_visuals: RwLock<HashMap<(u32, u32), ideck_core::VisualState>>,
}

impl MockSurface {
    pub fn new(id: SurfaceId, name: impl Into<String>) -> Arc<Self> {
        Self::with_id(id, name)
    }

    pub fn with_id(id: SurfaceId, name: impl Into<String>) -> Arc<Self> {
        let caps = SurfaceCapabilities::mock_3x5();
        Arc::new(Self {
            descriptor: SurfaceDescriptor {
                id,
                name: name.into(),
                capabilities: caps,
                backend: crate::SurfaceBackend::Mock,
            },
            input_tx: broadcast::channel(64).0,
            cell_visuals: RwLock::new(HashMap::new()),
        })
    }

    pub async fn cell_visuals(&self) -> HashMap<(u32, u32), ideck_core::VisualState> {
        self.cell_visuals.read().await.clone()
    }

    pub fn inject_input(&self, input: SurfaceInput) {
        let _ = self.input_tx.send(input);
    }

    pub fn subscribe_inputs(&self) -> broadcast::Receiver<SurfaceInput> {
        self.input_tx.subscribe()
    }
}

#[async_trait]
impl Surface for MockSurface {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        let mut visuals = self.cell_visuals.write().await;
        for update in updates {
            visuals.insert(
                (update.address.row, update.address.column),
                update.visual.clone(),
            );
        }
        Ok(())
    }
}

/// Registry of connected surfaces.
pub struct SurfaceManager {
    surfaces: RwLock<HashMap<SurfaceId, Arc<dyn Surface>>>,
    mock_inputs: RwLock<HashMap<SurfaceId, broadcast::Sender<SurfaceInput>>>,
    physical: RwLock<HashMap<SurfaceId, Arc<dyn PhysicalSurface>>>,
}

impl Default for SurfaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceManager {
    pub fn new() -> Self {
        Self {
            surfaces: RwLock::new(HashMap::new()),
            mock_inputs: RwLock::new(HashMap::new()),
            physical: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register_mock(&self, surface: Arc<MockSurface>) {
        let id = surface.descriptor.id;
        self.mock_inputs
            .write()
            .await
            .insert(id, surface.input_tx.clone());
        self.surfaces
            .write()
            .await
            .insert(id, surface as Arc<dyn Surface>);
    }

    pub async fn register_physical(&self, surface: Arc<dyn PhysicalSurface>) {
        let id = surface.surface_id();
        self.attach_physical(id, surface).await;
    }

    /// Attach a physical device at an existing surface id (replaces mock if present).
    pub async fn attach_physical(&self, surface_id: SurfaceId, surface: Arc<dyn PhysicalSurface>) {
        debug_assert_eq!(surface.surface_id(), surface_id);
        self.mock_inputs.write().await.remove(&surface_id);
        self.physical
            .write()
            .await
            .insert(surface_id, surface.clone());
        self.surfaces.write().await.insert(
            surface_id,
            Arc::new(ErasedPhysicalSurface(surface)),
        );
    }

    pub fn physical(&self, id: SurfaceId) -> Option<Arc<dyn PhysicalSurface>> {
        self.physical
            .try_read()
            .ok()
            .and_then(|m| m.get(&id).cloned())
    }

    pub async fn list(&self) -> Vec<SurfaceDescriptor> {
        self.surfaces
            .read()
            .await
            .values()
            .map(|s| s.descriptor().clone())
            .collect()
    }

    pub async fn get(&self, id: SurfaceId) -> Option<Arc<dyn Surface>> {
        self.surfaces.read().await.get(&id).cloned()
    }

    pub async fn render(&self, id: SurfaceId, updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        let surface = self
            .get(id)
            .await
            .ok_or(SurfaceError::NotFound(id))?;
        surface.render(updates).await
    }

    pub fn subscribe_mock_inputs(&self, id: SurfaceId) -> Option<broadcast::Receiver<SurfaceInput>> {
        self.mock_inputs
            .try_read()
            .ok()
            .and_then(|m| m.get(&id).map(|tx| tx.subscribe()))
    }

    pub async fn unregister(&self, id: SurfaceId) {
        self.surfaces.write().await.remove(&id);
        self.mock_inputs.write().await.remove(&id);
        self.physical.write().await.remove(&id);
    }
}
