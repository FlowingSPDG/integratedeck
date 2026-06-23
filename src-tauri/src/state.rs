use ideck_comp_host::CompModuleRuntime;
use ideck_core::{PageId, Profile, SurfaceId, VariableStore};
use ideck_sd_host::PluginSupervisor;
use ideck_surface::SurfaceManager;
use ideck_surface::SurfaceDriverRegistry;

use std::collections::HashMap;

use ideck_bridge::{CompSurfaceBridge, SdSurfaceBridge};

use crate::hub::{DeviceEventBus, RoutingTable};

pub struct AppStateInner {
    pub profile: Profile,
    pub surfaces: SurfaceManager,
    pub surface_drivers: SurfaceDriverRegistry,
    pub connections: ideck_comp_host::ConnectionRegistry,
    pub variables: VariableStore,
    pub comp_runtime: Option<CompModuleRuntime>,
    pub sd_supervisor: PluginSupervisor,
    pub bridges: HashMap<String, SdSurfaceBridge>,
    pub comp_bridge: Option<CompSurfaceBridge>,
    /// Plugin-driven visuals keyed by (surface, page, row, col).
    pub cell_visuals: HashMap<(SurfaceId, PageId, u32, u32), ideck_core::VisualState>,
    pub device_bus: DeviceEventBus,
    pub routing: RoutingTable,
    pub global_settings: HashMap<String, serde_json::Value>,
    /// Current visible page per physical surface (folder navigation).
    pub surface_pages: HashMap<SurfaceId, PageId>,
    /// Folder navigation stack per surface.
    pub page_stacks: HashMap<SurfaceId, Vec<PageId>>,
}

impl AppStateInner {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            surfaces: SurfaceManager::new(),
            surface_drivers: SurfaceDriverRegistry::new(),
            connections: ideck_comp_host::ConnectionRegistry::default(),
            variables: VariableStore::default(),
            comp_runtime: None,
            sd_supervisor: PluginSupervisor::new(),
            bridges: HashMap::new(),
            comp_bridge: None,
            cell_visuals: HashMap::new(),
            device_bus: DeviceEventBus::new(),
            routing: RoutingTable::default(),
            global_settings: HashMap::new(),
            surface_pages: HashMap::new(),
            page_stacks: HashMap::new(),
        }
    }

    pub fn page_for_surface(&self, surface_id: SurfaceId) -> Option<PageId> {
        self.surface_pages
            .get(&surface_id)
            .copied()
            .or(self.profile.active_page_id)
    }

    pub fn ensure_surface_page(&mut self, surface_id: SurfaceId) {
        if self.surface_pages.contains_key(&surface_id) {
            return;
        }
        if let Some(page_id) = self.profile.active_page_id {
            self.surface_pages.insert(surface_id, page_id);
        }
    }
}
