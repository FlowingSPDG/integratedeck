use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};

use crate::{CellUpdate, SurfaceCapabilities, SurfaceDescriptor, SurfaceInput};
use ideck_core::SurfaceId;

#[derive(Debug, Error)]
pub enum SurfaceError {
    #[error("surface not found: {0:?}")]
    NotFound(SurfaceId),
    #[error("{0}")]
    Other(String),
}

/// Physical or virtual control surface.
#[async_trait]
pub trait Surface: Send + Sync {
    fn descriptor(&self) -> &SurfaceDescriptor;
    async fn render(&self, updates: &[CellUpdate]) -> Result<(), SurfaceError>;
}

/// Mock surface for development and tests.
pub struct MockSurface {
    descriptor: SurfaceDescriptor,
    input_tx: broadcast::Sender<SurfaceInput>,
}

impl MockSurface {
    pub fn new(id: SurfaceId, name: impl Into<String>) -> Arc<Self> {
        let caps = SurfaceCapabilities::mock_3x5();
        Arc::new(Self {
            descriptor: SurfaceDescriptor {
                id,
                name: name.into(),
                capabilities: caps,
                backend: crate::SurfaceBackend::Mock,
            },
            input_tx: broadcast::channel(64).0,
        })
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

    async fn render(&self, _updates: &[CellUpdate]) -> Result<(), SurfaceError> {
        Ok(())
    }
}

/// Registry of connected surfaces.
pub struct SurfaceManager {
    surfaces: RwLock<HashMap<SurfaceId, Arc<dyn Surface>>>,
    mock_inputs: RwLock<HashMap<SurfaceId, broadcast::Sender<SurfaceInput>>>,
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

    pub async fn register(&self, surface: Arc<dyn Surface>) {
        let id = surface.descriptor().id;
        self.surfaces.write().await.insert(id, surface);
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
        // Called synchronously from sync context in tests; use try_read in production orchestrator
        self.mock_inputs
            .try_read()
            .ok()
            .and_then(|m| m.get(&id).map(|tx| tx.subscribe()))
    }
}
