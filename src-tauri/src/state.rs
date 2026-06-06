use ideck_core::{Profile, VariableStore};
use ideck_comp_host::{ConnectionRegistry, SidecarClient};
use ideck_sd_host::StreamDeckBroker;
use ideck_surface::SurfaceManager;

use std::collections::HashMap;
use std::sync::Arc;

use ideck_bridge::SdSurfaceBridge;

pub struct AppStateInner {
    pub profile: Profile,
    pub surfaces: SurfaceManager,
    pub connections: ConnectionRegistry,
    pub variables: VariableStore,
    pub sidecar: Option<SidecarClient>,
    pub sd_broker: Option<Arc<StreamDeckBroker>>,
    pub bridge: Option<SdSurfaceBridge>,
    pub cell_visuals: HashMap<(u32, u32), ideck_core::VisualState>,
    pub pi_port: Option<u16>,
}

impl AppStateInner {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            surfaces: SurfaceManager::new(),
            connections: ConnectionRegistry::default(),
            variables: VariableStore::default(),
            sidecar: None,
            sd_broker: None,
            bridge: None,
            cell_visuals: HashMap::new(),
            pi_port: None,
        }
    }
}
