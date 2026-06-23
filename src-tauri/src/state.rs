use ideck_comp_host::CompModuleRuntime;
use ideck_core::{PageId, Profile, SurfaceId, VariableStore};
use ideck_sd_host::PluginSupervisor;
use ideck_surface::{SurfaceInput, SurfaceManager};
use ideck_surface::SurfaceDriverRegistry;
use tracing::warn;

use std::collections::HashMap;

use ideck_bridge::{CompSurfaceBridge, SdSurfaceBridge};

use crate::hub::{DeviceEventBus, RoutingTable};

fn resolve_sd_key_target(
    inner: &AppStateInner,
    surface_id: SurfaceId,
    row: u32,
    column: u32,
) -> Option<(String, String)> {
    let key = (surface_id, row, column);
    if let Some(plugin_uuid) = inner.routing.cell_to_plugin.get(&key) {
        let context = inner.routing.cell_to_context.get(&key)?.clone();
        return Some((plugin_uuid.clone(), context));
    }
    for (plugin_uuid, bridge) in &inner.bridges {
        if let Some(context) = bridge.context_by_cell.get(&key) {
            return Some((plugin_uuid.clone(), context.clone()));
        }
    }
    if let Some(context) = inner.routing.cell_to_context.get(&key) {
        let loaded = inner.sd_supervisor.plugin_uuids();
        let plugin_uuid = SdSurfaceBridge::plugin_uuid_for_context(context, &loaded)?;
        return Some((plugin_uuid, context.clone()));
    }
    None
}

/// Forward physical input to Stream Deck plugins without holding the orchestrator write lock.
pub async fn forward_sd_input(
    inner: &AppStateInner,
    surface_id: SurfaceId,
    input: &SurfaceInput,
) {
    match input {
        SurfaceInput::KeyDown { address } => {
            let Some((plugin_uuid, context)) =
                resolve_sd_key_target(inner, surface_id, address.row, address.column)
            else {
                return;
            };
            let Some(broker) = inner.sd_supervisor.get(&plugin_uuid) else {
                warn!("SD keyDown: plugin not loaded ({plugin_uuid})");
                return;
            };
            if !broker.has_context(&context).await {
                warn!("SD keyDown: context not registered ({context}) — bind or open PI first");
                return;
            }
            if let Err(e) = broker.key_down(&context).await {
                warn!("SD keyDown ({context}): {e}");
            }
        }
        SurfaceInput::KeyUp { address } => {
            let Some((plugin_uuid, context)) =
                resolve_sd_key_target(inner, surface_id, address.row, address.column)
            else {
                return;
            };
            let Some(broker) = inner.sd_supervisor.get(&plugin_uuid) else {
                warn!("SD keyUp: plugin not loaded ({plugin_uuid})");
                return;
            };
            if !broker.has_context(&context).await {
                warn!("SD keyUp: context not registered ({context})");
                return;
            }
            if let Err(e) = broker.key_up(&context).await {
                warn!("SD keyUp ({context}): {e}");
            }
        }
        SurfaceInput::EncoderRotate { .. } | SurfaceInput::EncoderPress { .. } => {
            for (plugin_uuid, bridge) in &inner.bridges {
                let Some(broker) = inner.sd_supervisor.get(plugin_uuid) else {
                    continue;
                };
                match bridge.surface_input_to_sd_event(surface_id, input) {
                    Some((context, "dialRotate")) => {
                        if let SurfaceInput::EncoderRotate { ticks, pressed, .. } = input {
                            if let Err(e) = broker.dial_rotate(&context, *ticks, *pressed).await {
                                warn!("SD dialRotate ({context}): {e}");
                            }
                        }
                        return;
                    }
                    Some((context, "dialPress")) if broker.key_down(&context).await.is_ok() => {
                        let _ = broker.key_up(&context).await;
                        return;
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

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
