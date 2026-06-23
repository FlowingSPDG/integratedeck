use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ideck_bridge::SdSurfaceBridge;
use ideck_bridge::CompSurfaceBridge;
use ideck_comp_host::{
    scan_module_dirs, CompModuleRuntime, ConnectionRecord, HostEvent,
};
use ideck_core::{
    ActionInstanceId, Binding, BindingKind, FolderSettings, MultiActionStep, Page, PageId,
    Profile, Slot, SlotAppearance, SlotId, SurfaceId, SwitchPageSettings, VisualState,
    BACK_TO_PARENT, MULTI_ACTION, OPEN_FOLDER, PLUGIN_ID as BUILTIN_PLUGIN_ID, SWITCH_PAGE,
    action_display_name, image_from_base64, merge_visual, solid_key_png,
};
use ideck_sd_host::{
    scan_sd_plugins_roots, ActionContext, BrokerEvent, effective_plugin_uuid, ImageSetResult,
    LoadedSdPlugin, PluginSupervisor, StreamDeckBroker, StreamDeckManifest,
};
use ideck_surface::{
    CellAddress, CellUpdate, DiscoveredDevice, HidDeviceDescriptor, MockSurface, PhysicalSurface,
    StreamDeckHidSurface, SurfaceCapabilities, SurfaceDriverManifest, SurfaceInput, DRIVER_ID,
};
use serde_json::json;
use tauri::AppHandle;
use tauri::Emitter;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::hub::{HubEvents, PluginStatusPayload, SettingsChangedPayload, VisualUpdatedPayload};
use crate::paths;
use crate::settings_store::{self, AppGlobalSettings, DeviceRecord};
use crate::state::AppStateInner;

pub struct Orchestrator {
    app: AppHandle,
    inner: AppStateInner,
    hid_serials: std::collections::HashMap<String, SurfaceId>,
    hid_meta: std::collections::HashMap<SurfaceId, HidSurfaceMeta>,
    hid_task_handles: std::collections::HashMap<SurfaceId, Vec<tokio::task::JoinHandle<()>>>,
    last_pi_context: Option<String>,
    last_pi_plugin_uuid: Option<String>,
}

#[derive(Debug, Clone)]
struct HidSurfaceMeta {
    serial: String,
    kind: String,
    product: String,
}

impl Orchestrator {
    pub async fn new(app: &AppHandle) -> anyhow::Result<Self> {
        let mut orch = Self::new_core(app).await?;
        orch.load_connections();
        orch.spawn_comp_module_engine().await;
        Ok(orch)
    }

    /// Post-startup: restore connections, rehydrate bindings, reconnect HID devices.
    pub fn start_background_init(state: Arc<RwLock<Self>>) {
        tauri::async_runtime::spawn(async move {
            {
                let mut o = state.write().await;
                o.restore_companion_connections().await;
            }
            {
                let mut o = state.write().await;
                if let Err(e) = o.rehydrate_profile_bindings(state.clone()).await {
                    warn!("profile rehydrate failed: {e}");
                }
            }
            Self::auto_connect_available_devices(state.clone()).await;
        });
    }

    pub fn start_usb_watch(state: Arc<RwLock<Self>>) {
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                Self::auto_connect_available_devices(state.clone()).await;
            }
        });
    }

    /// Register newly discovered USB devices and connect any that are not yet active.
    pub async fn auto_connect_available_devices(state: Arc<RwLock<Self>>) {
        let targets: Vec<(String, String)> = {
            let o = state.read().await;
            let _ = o.sync_device_registry();
            o.scan_hid_devices()
                .unwrap_or_default()
                .into_iter()
                .filter(|d| !o.hid_serials.contains_key(&d.serial))
                .map(|d| (d.serial.clone(), d.kind.clone()))
                .collect()
        };

        for (serial, kind) in targets {
            match Self::connect_hid_with_loops(state.clone(), serial.clone(), kind).await {
                Ok(_) => info!("auto-connected HID device {serial}"),
                Err(e) => warn!("auto-connect HID {serial} failed: {e}"),
            }
        }
    }

    /// Merge USB scan results into the persisted device registry.
    fn sync_device_registry(&self) -> Result<Vec<settings_store::DeviceRecord>, String> {
        let mut records = settings_store::load_device_registry();
        let scanned = self.scan_hid_devices().unwrap_or_default();
        let mut changed = false;

        for device in &scanned {
            let key = settings_store::device_key(&device.kind, &device.serial);
            if let Some(existing) = records.iter_mut().find(|r| r.device_key == key) {
                existing.last_seen = settings_store::now_iso();
                if existing.product != device.product {
                    existing.product = device.product.clone();
                    changed = true;
                }
            } else {
                records.push(settings_store::DeviceRecord {
                    device_key: key,
                    serial: device.serial.clone(),
                    kind: device.kind.clone(),
                    product: device.product.clone(),
                    label: device.product.clone(),
                    brightness: 50,
                    last_seen: settings_store::now_iso(),
                    surface_id: self
                        .hid_serials
                        .get(&device.serial)
                        .map(|id| id.0.to_string()),
                });
                changed = true;
            }
        }

        if changed {
            settings_store::save_device_registry(&records)?;
        }
        Ok(records)
    }

    pub fn resolve_device_record(
        &self,
        device_key: &str,
    ) -> Result<settings_store::DeviceRecord, String> {
        let records = self.sync_device_registry()?;
        let record = records
            .iter()
            .find(|r| r.device_key == device_key)
            .ok_or_else(|| "デバイスが見つかりません。".to_string())?;
        let scanned = self.scan_hid_devices().unwrap_or_default();
        if !scanned
            .iter()
            .any(|d| d.serial == record.serial && d.kind == record.kind)
        {
            return Err(
                "デバイスが USB に接続されていません。ケーブルと Elgato 公式アプリの終了を確認してください。"
                    .into(),
            );
        }
        Ok(record.clone())
    }

    pub async fn connect_hid_with_loops(
        state: Arc<RwLock<Self>>,
        serial: String,
        kind: String,
    ) -> Result<ConnectHidResult, String> {
        let (result, surface) = {
            let mut o = state.write().await;
            o.connect_hid_device(serial, kind).await?
        };
        let surface_id =
            SurfaceId(uuid::Uuid::parse_str(&result.surface_id).map_err(|e| e.to_string())?);
        Self::attach_hid_input_loop(&state, surface_id, surface).await;
        {
            let mut o = state.write().await;
            o.finalize_hid_connection(surface_id, state.clone()).await?;
        }
        {
            let o = state.read().await;
            HubEvents::emit_surfaces_changed(&o.app, Some(result.surface_id.clone()));
        }
        Ok(result)
    }

    async fn attach_hid_input_loop(
        state: &Arc<RwLock<Self>>,
        surface_id: SurfaceId,
        surface: Arc<dyn PhysicalSurface>,
    ) {
        let state_clone = state.clone();
        let mut input_rx = surface.subscribe_inputs();
        let reader = surface.clone().spawn_input_loop();

        let forward = tokio::spawn(async move {
            while let Ok(input) = input_rx.recv().await {
                let mut o = state_clone.write().await;
                o.apply_surface_input(surface_id, input).await.ok();
            }
        });

        let mut o = state.write().await;
        o.store_hid_tasks(surface_id, vec![reader, forward]);
    }

    pub async fn new_core(app: &AppHandle) -> anyhow::Result<Self> {
        paths::ensure_dirs()?;
        let profile_path = paths::profiles_dir().join("default.json");
        let profile = if profile_path.exists() {
            let data = std::fs::read_to_string(&profile_path)?;
            serde_json::from_str::<Profile>(&data).unwrap_or_else(|_| Profile::new("Default"))
        } else {
            Profile::new("Default")
        };

        let inner = AppStateInner::new(profile);
        for assignment in inner.profile.surfaces.clone() {
            let mock = MockSurface::with_id(assignment.surface_id, assignment.label.clone());
            inner.surfaces.register_mock(mock).await;
        }

        Ok(Self {
            app: app.clone(),
            inner,
            hid_serials: std::collections::HashMap::new(),
            hid_meta: std::collections::HashMap::new(),
            hid_task_handles: std::collections::HashMap::new(),
            last_pi_context: None,
            last_pi_plugin_uuid: None,
        })
    }

    fn load_connections(&mut self) {
        let path = paths::connections_file();
        if !path.exists() {
            return;
        }
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(records) = serde_json::from_str::<Vec<ConnectionRecord>>(&data) {
                for record in records {
                    self.inner.connections.add(record);
                }
            }
        }
    }

    fn save_connections(&self) -> anyhow::Result<()> {
        let path = paths::connections_file();
        let data = serde_json::to_string_pretty(&self.inner.connections.list())?;
        std::fs::write(path, data)?;
        Ok(())
    }

    async fn spawn_comp_module_engine(&mut self) {
        let modules_dir = paths::companion_modules_dir();
        match CompModuleRuntime::spawn(&modules_dir).await {
            Ok(runtime) => {
                info!("companion module engine started");
                self.inner.comp_runtime = Some(runtime);
            }
            Err(e) => warn!("companion module engine not started: {e}"),
        }
    }

    pub fn start_companion_event_drain(self_arc: Arc<RwLock<Self>>) {
        tauri::async_runtime::spawn(async move {
            let rx = {
                let o = self_arc.read().await;
                o.inner.comp_runtime.as_ref().map(|r| r.subscribe_events())
            };
            let Some(mut rx) = rx else { return };
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        let mut o = self_arc.write().await;
                        o.apply_companion_event(ev).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
    }

    async fn apply_companion_event(&mut self, ev: HostEvent) {
        match ev.event.as_str() {
            "feedback.updated" => {
                let bridge = match &self.inner.comp_bridge {
                    Some(b) => b,
                    None => return,
                };
                let values = ev
                    .data
                    .get("values")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                for item in values {
                    let control_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let style = item.get("style").cloned().unwrap_or(json!({}));
                    let visual = CompSurfaceBridge::parse_feedback_style(&style);
                    if let Some(update) = bridge.feedback_to_update(control_id, visual.clone()) {
                        let page_id = self
                            .inner
                            .profile
                            .active_page_id
                            .unwrap_or_else(PageId::new);
                        self.inner.cell_visuals.insert(
                            (page_id, update.address.row, update.address.column),
                            visual.clone(),
                        );
                        let surface_id =
                            self.surface_id_for_cell(update.address.row, update.address.column);
                        self.inner
                            .surfaces
                            .render(surface_id, &[update.clone()])
                            .await
                            .ok();
                        HubEvents::emit_visual(
                            &self.app,
                            VisualUpdatedPayload {
                                row: update.address.row,
                                column: update.address.column,
                                visual,
                            },
                        );
                    }
                }
            }
            "variable.updated" => {
                if let Some(values) = ev.data.get("values").and_then(|v| v.as_array()) {
                    for item in values {
                        if let (Some(id), Some(value)) = (
                            item.get("id").and_then(|v| v.as_str()),
                            item.get("value").and_then(|v| v.as_str()),
                        ) {
                            self.inner.variables.set(id, value);
                        }
                    }
                }
            }
            "definitions.updated" => {
                let _ = self.app.emit(HubEvents::DEFINITIONS_UPDATED, ev.data);
            }
            "connection.status" => {
                let _ = self.app.emit("connection-status", ev.data);
            }
            "connection.config" => {
                if let (Some(connection_id), Some(config)) = (
                    ev.data.get("connectionId").and_then(|v| v.as_str()),
                    ev.data.get("config"),
                ) {
                    if let Some(record) = self.inner.connections.get(connection_id) {
                        let mut updated = record.clone();
                        updated.config = config.clone();
                        self.inner.connections.add(updated);
                        let _ = self.save_connections();
                    }
                }
            }
            _ => {}
        }
    }

    async fn restore_companion_connections(&mut self) {
        let records: Vec<ConnectionRecord> = self.inner.connections.list();
        let Some(runtime) = self.inner.comp_runtime.as_ref() else {
            return;
        };
        for record in records {
            if !record.enabled {
                continue;
            }
            let resp = runtime
                .request(
                    "connection.add",
                    json!({
                        "id": record.id,
                        "moduleId": record.module_id,
                        "label": record.label,
                        "config": record.config,
                        "secrets": {}
                    }),
                )
                .await;
            if let Err(e) = resp {
                warn!("restore connection {} failed: {e}", record.id);
            }
        }
    }

    fn find_sd_plugin_path(&self, plugin_uuid: &str) -> Option<PathBuf> {
        let scan = self.scan_plugins().ok()?;
        scan.streamdeck.into_iter().find_map(|entry| {
            let path = PathBuf::from(&entry.path);
            let uuid = entry.uuid.as_deref().unwrap_or("");
            if uuid == plugin_uuid
                || effective_plugin_uuid(&path, entry.uuid.as_deref()) == plugin_uuid
            {
                Some(path)
            } else {
                None
            }
        })
    }

    fn collect_all_slot_bindings(&self) -> Vec<(SlotId, ideck_core::Slot)> {
        let mut out = Vec::new();
        for page in &self.inner.profile.pages {
            for (slot_id, slot) in &page.slots {
                if slot.binding.is_some() {
                    out.push((*slot_id, slot.clone()));
                }
            }
        }
        out
    }

    pub async fn rehydrate_profile_bindings(
        &mut self,
        orch: Arc<RwLock<Self>>,
    ) -> anyhow::Result<()> {
        let bindings = self.collect_all_slot_bindings();
        let mut sd_uuids = std::collections::HashSet::new();
        for (_, slot) in &bindings {
            if let Some(Binding {
                kind:
                    BindingKind::StreamDeck {
                        plugin_uuid, ..
                    },
            }) = &slot.binding
            {
                sd_uuids.insert(plugin_uuid.clone());
            }
        }

        for plugin_uuid in sd_uuids {
            if self.inner.sd_supervisor.get(&plugin_uuid).is_some() {
                continue;
            }
            if let Some(path) = self.find_sd_plugin_path(&plugin_uuid) {
                self.load_sd_plugin(path, orch.clone()).await?;
            } else {
                warn!("SD plugin not found for rehydrate: {plugin_uuid}");
            }
        }

        for (slot_id, slot) in bindings {
            if self.inner.routing.slot_to_context.contains_key(&slot_id)
                || self
                    .inner
                    .routing
                    .slot_to_companion
                    .contains_key(&slot_id)
            {
                continue;
            }
            match slot.binding {
                Some(Binding {
                    kind:
                        BindingKind::StreamDeck {
                            plugin_uuid,
                            action_uuid,
                            instance_id,
                            settings,
                        },
                }) => {
                    self.restore_sd_binding(
                        slot_id,
                        &plugin_uuid,
                        &action_uuid,
                        instance_id,
                        settings,
                        slot.locator.surface_id,
                        slot.locator.row,
                        slot.locator.column,
                    )
                    .await?;
                }
                Some(Binding {
                    kind:
                        BindingKind::Companion {
                            connection_id,
                            action_id,
                            ..
                        },
                }) => {
                    self.restore_companion_binding(
                        slot_id,
                        &connection_id,
                        &action_id,
                        slot.locator.row,
                        slot.locator.column,
                    );
                }
                None => {}
                Some(Binding {
                    kind: BindingKind::BuiltIn { .. } | BindingKind::MultiAction { .. },
                }) => {}
            }
        }
        Ok(())
    }

    async fn restore_sd_binding(
        &mut self,
        slot_id: SlotId,
        plugin_uuid: &str,
        action_uuid: &str,
        instance_id: ActionInstanceId,
        settings: serde_json::Value,
        surface_id: SurfaceId,
        row: u32,
        col: u32,
    ) -> anyhow::Result<()> {
        let context =
            SdSurfaceBridge::build_context_id(plugin_uuid, action_uuid, &instance_id.0);

        if let Some(bridge) = self.inner.bridges.get_mut(plugin_uuid) {
            bridge.register_cell(surface_id, row, col, context.clone());
        }
        self.inner
            .routing
            .register_sd_cell(surface_id, row, col, context.clone());
        self.inner
            .routing
            .register_sd_slot(slot_id, context.clone());

        if let Some(broker) = self.inner.sd_supervisor.get(plugin_uuid) {
            broker
                .register_context(ActionContext {
                    context: context.clone(),
                    action_uuid: action_uuid.to_string(),
                    settings,
                    coordinates: (col, row),
                })
                .await;
            broker.will_appear(&context).await?;
        }
        Ok(())
    }

    fn restore_companion_binding(
        &mut self,
        slot_id: SlotId,
        connection_id: &str,
        action_id: &str,
        row: u32,
        col: u32,
    ) {
        self.inner.routing.register_companion_slot(
            slot_id,
            connection_id.to_string(),
            action_id.to_string(),
        );
        if self.inner.comp_bridge.is_none() {
            self.inner.comp_bridge = Some(CompSurfaceBridge::new());
        }
        if let Some(bridge) = self.inner.comp_bridge.as_mut() {
            bridge.register_cell(row, col, format!("{connection_id}:{action_id}"));
        }
    }

    fn device_key_for_surface(&self, surface_id: SurfaceId) -> Option<String> {
        for (sid, meta) in &self.hid_meta {
            if *sid == surface_id {
                return Some(settings_store::device_key(&meta.kind, &meta.serial));
            }
        }
        let registry = settings_store::load_device_registry();
        registry
            .iter()
            .find(|r| {
                r.surface_id
                    .as_ref()
                    .and_then(|id| uuid::Uuid::parse_str(id).ok())
                    .map(SurfaceId)
                    == Some(surface_id)
            })
            .map(|r| r.device_key.clone())
    }

    async fn auto_export_device_preset_for_surface(&self, surface_id: SurfaceId) {
        if let Some(device_key) = self.device_key_for_surface(surface_id) {
            let _ = self.export_device_preset(&device_key);
        }
    }

    async fn persist_slot_changes(&mut self, surface_id: SurfaceId) -> anyhow::Result<()> {
        self.save_profile().await?;
        self.auto_export_device_preset_for_surface(surface_id).await;
        Ok(())
    }

    pub fn page_id_for_slot(&self, slot_id: SlotId) -> Option<PageId> {
        self.inner
            .profile
            .pages
            .iter()
            .find(|p| p.slots.contains_key(&slot_id))
            .map(|p| p.id)
    }

    fn effective_visual(&self, page_id: PageId, slot: &Slot) -> VisualState {
        let key = (page_id, slot.locator.row, slot.locator.column);
        let mut visual = self
            .inner
            .cell_visuals
            .get(&key)
            .cloned()
            .unwrap_or_default();
        slot.appearance.apply_to_visual(&mut visual);
        visual
    }

    fn page_id_for_context(&self, context: &str) -> Option<PageId> {
        let slot_id = self.slot_id_for_context(context)?;
        self.page_id_for_slot(slot_id)
    }

    async fn refresh_page_visuals(&mut self, page_id: PageId, surface_id: SurfaceId) {
        let Some(page) = self.inner.profile.pages.iter().find(|p| p.id == page_id) else {
            return;
        };
        let mut updates = Vec::new();
        for slot in page.slots.values() {
            if slot.locator.surface_id != surface_id {
                continue;
            }
            let visual = self.effective_visual(page_id, slot);
            updates.push(CellUpdate {
                address: CellAddress {
                    row: slot.locator.row,
                    column: slot.locator.column,
                },
                visual,
            });
        }
        if !updates.is_empty() {
            self.inner
                .surfaces
                .render(surface_id, &updates)
                .await
                .ok();
        }
    }

    async fn navigate_surface_to_page(
        &mut self,
        surface_id: SurfaceId,
        page_id: PageId,
        push_stack: bool,
    ) -> anyhow::Result<()> {
        if !self.inner.profile.pages.iter().any(|p| p.id == page_id) {
            anyhow::bail!("page not found");
        }
        self.inner.ensure_surface_page(surface_id);
        if push_stack {
            if let Some(current) = self.inner.surface_pages.get(&surface_id).copied() {
                if current != page_id {
                    self.inner
                        .page_stacks
                        .entry(surface_id)
                        .or_default()
                        .push(current);
                }
            }
        }
        let old_page = self.inner.surface_pages.get(&surface_id).copied();
        if let Some(old) = old_page {
            if old != page_id {
                self.sd_will_disappear_for_surface_page(surface_id, old)
                    .await;
            }
        }
        self.inner.surface_pages.insert(surface_id, page_id);
        self.sync_sd_routing_for_surface_page(surface_id, page_id)
            .await;
        self.refresh_page_visuals(page_id, surface_id).await;
        Ok(())
    }

    async fn sd_will_disappear_for_surface_page(
        &mut self,
        surface_id: SurfaceId,
        page_id: PageId,
    ) {
        let contexts: Vec<(String, String)> = {
            let Some(page) = self.inner.profile.pages.iter().find(|p| p.id == page_id) else {
                return;
            };
            page.slots
                .values()
                .filter(|slot| slot.locator.surface_id == surface_id)
                .filter_map(|slot| {
                    let binding = slot.binding.as_ref()?;
                    let BindingKind::StreamDeck {
                        plugin_uuid,
                        action_uuid,
                        instance_id,
                        ..
                    } = &binding.kind
                    else {
                        return None;
                    };
                    let context = SdSurfaceBridge::build_context_id(
                        plugin_uuid,
                        action_uuid,
                        &instance_id.0,
                    );
                    Some((plugin_uuid.clone(), context))
                })
                .collect()
        };
        for (plugin_uuid, context) in contexts {
            if let Some(broker) = self.inner.sd_supervisor.get(&plugin_uuid) {
                broker.will_disappear(&context).await.ok();
            }
        }
    }

    async fn sync_sd_routing_for_surface_page(
        &mut self,
        surface_id: SurfaceId,
        page_id: PageId,
    ) {
        let Some(page) = self.inner.profile.pages.iter().find(|p| p.id == page_id) else {
            return;
        };
        for slot in page.slots.values() {
            if slot.locator.surface_id != surface_id {
                continue;
            }
            let Some(Binding {
                kind:
                    BindingKind::StreamDeck {
                        plugin_uuid,
                        action_uuid,
                        instance_id,
                        settings,
                    },
            }) = &slot.binding
            else {
                continue;
            };
            let context =
                SdSurfaceBridge::build_context_id(plugin_uuid, action_uuid, &instance_id.0);
            if let Some(bridge) = self.inner.bridges.get_mut(plugin_uuid) {
                let cell_key = (surface_id, slot.locator.row, slot.locator.column);
                if let Some(old) = bridge.context_by_cell.get(&cell_key) {
                    if old != &context {
                        bridge.cell_by_context.remove(old);
                    }
                }
                bridge.register_cell(
                    surface_id,
                    slot.locator.row,
                    slot.locator.column,
                    context.clone(),
                );
            }
            self.inner.routing.register_sd_cell(
                surface_id,
                slot.locator.row,
                slot.locator.column,
                context.clone(),
            );
            self.inner.routing.register_sd_slot(slot.id, context.clone());
            if let Some(broker) = self.inner.sd_supervisor.get(plugin_uuid) {
                broker
                    .register_context(ActionContext {
                        context: context.clone(),
                        action_uuid: action_uuid.clone(),
                        settings: settings.clone(),
                        coordinates: (slot.locator.column, slot.locator.row),
                    })
                    .await;
                broker.will_appear(&context).await.ok();
            }
        }
    }

    async fn navigate_surface_back(&mut self, surface_id: SurfaceId) -> anyhow::Result<()> {
        let parent = self
            .inner
            .page_stacks
            .get_mut(&surface_id)
            .and_then(|stack| stack.pop());
        let page_id = parent.or_else(|| {
            let current = self.inner.surface_pages.get(&surface_id).copied()?;
            let page = self.inner.profile.pages.iter().find(|p| p.id == current)?;
            page.parent_page_id
        });
        let Some(page_id) = page_id else {
            return Ok(());
        };
        self.inner.surface_pages.insert(surface_id, page_id);
        self.sync_sd_routing_for_surface_page(surface_id, page_id)
            .await;
        self.refresh_page_visuals(page_id, surface_id).await;
        Ok(())
    }

    fn create_folder_child_page(&mut self, parent_page_id: PageId, folder_name: &str) -> PageId {
        let child_id = PageId::new();
        let sibling_count = self
            .inner
            .profile
            .pages
            .iter()
            .filter(|p| p.parent_page_id == Some(parent_page_id))
            .count();
        let name = if folder_name.is_empty() {
            format!("Folder {}", sibling_count + 1)
        } else {
            folder_name.to_string()
        };
        self.inner.profile.pages.push(Page {
            id: child_id,
            name,
            slots: Default::default(),
            parent_page_id: Some(parent_page_id),
        });
        child_id
    }

    pub async fn update_slot_appearance(
        &mut self,
        slot_id: SlotId,
        title: Option<String>,
        default_image_base64: Option<String>,
        clear_image: bool,
    ) -> anyhow::Result<()> {
        let page_id = self
            .page_id_for_slot(slot_id)
            .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
        let surface_id = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            page.slots
                .get(&slot_id)
                .map(|s| s.locator.surface_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?
        };

        if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
            if let Some(slot) = page.slots.get_mut(&slot_id) {
                slot.appearance.title = title.filter(|t| !t.trim().is_empty());
                if clear_image {
                    slot.appearance.default_image = None;
                } else if let Some(b64) = default_image_base64 {
                    if let Some(img) = image_from_base64(&b64, ideck_core::ImageFormat::Png) {
                        slot.appearance.default_image = Some(img);
                    }
                }
            }
        }

        self.persist_slot_changes(surface_id).await?;
        let (row, col, visual) = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            let slot = page
                .slots
                .get(&slot_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
            let visual = self.effective_visual(page_id, slot);
            (slot.locator.row, slot.locator.column, visual)
        };
        self.inner
            .cell_visuals
            .insert((page_id, row, col), visual.clone());
        HubEvents::emit_visual(
            &self.app,
            VisualUpdatedPayload {
                row,
                column: col,
                visual: visual.clone(),
            },
        );
        self.refresh_page_visuals(page_id, surface_id).await;
        Ok(())
    }

    pub async fn bind_slot_builtin(
        &mut self,
        slot_id: SlotId,
        action_id: String,
    ) -> anyhow::Result<()> {
        self.unbind_slot(slot_id).await.ok();

        let page_id = self
            .inner
            .profile
            .active_page_id
            .ok_or_else(|| anyhow::anyhow!("no active page"))?;
        let surface_id = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            page.slots
                .get(&slot_id)
                .map(|s| s.locator.surface_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?
        };

        let settings = match action_id.as_str() {
            OPEN_FOLDER => {
                let child_id = self.create_folder_child_page(page_id, "Folder");
                json!({ "childPageId": child_id.0.to_string() })
            }
            MULTI_ACTION => json!({}),
            BACK_TO_PARENT | SWITCH_PAGE => json!({}),
            _ => anyhow::bail!("unknown built-in action: {action_id}"),
        };

        if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
            if let Some(slot) = page.slots.get_mut(&slot_id) {
                if action_id == MULTI_ACTION {
                    slot.binding = Some(Binding::multi_action(Vec::new(), 200));
                } else {
                    slot.binding = Some(Binding::built_in(action_id, settings));
                }
            }
        }

        self.persist_slot_changes(surface_id).await?;
        Ok(())
    }

    pub async fn update_multi_action(
        &mut self,
        slot_id: SlotId,
        steps: Vec<MultiActionStep>,
        delay_ms: Option<u32>,
    ) -> anyhow::Result<()> {
        let page_id = self
            .page_id_for_slot(slot_id)
            .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
        let surface_id = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            page.slots
                .get(&slot_id)
                .map(|s| s.locator.surface_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?
        };

        if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
            if let Some(slot) = page.slots.get_mut(&slot_id) {
                let delay = delay_ms.unwrap_or(200);
                slot.binding = Some(Binding::multi_action(steps, delay));
            }
        }
        self.persist_slot_changes(surface_id).await?;
        Ok(())
    }

    pub async fn set_switch_page_target(
        &mut self,
        slot_id: SlotId,
        target_page_id: PageId,
    ) -> anyhow::Result<()> {
        let page_id = self
            .page_id_for_slot(slot_id)
            .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
        let surface_id = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            page.slots
                .get(&slot_id)
                .map(|s| s.locator.surface_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?
        };
        let target_name = self
            .inner
            .profile
            .pages
            .iter()
            .find(|p| p.id == target_page_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();

        if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
            if let Some(slot) = page.slots.get_mut(&slot_id) {
                if let Some(Binding {
                    kind: BindingKind::BuiltIn { action_id, settings },
                }) = &mut slot.binding
                {
                    if action_id == SWITCH_PAGE {
                        *settings = json!({
                            "targetPageId": target_page_id.0.to_string(),
                            "targetPageName": target_name,
                        });
                    }
                }
            }
        }
        self.persist_slot_changes(surface_id).await?;
        Ok(())
    }

    async fn execute_binding_leaf(&mut self, kind: &BindingKind) -> anyhow::Result<()> {
        match kind {
            BindingKind::StreamDeck {
                plugin_uuid,
                action_uuid,
                instance_id,
                settings,
            } => {
                let context =
                    SdSurfaceBridge::build_context_id(plugin_uuid, action_uuid, &instance_id.0);
                let broker = self
                    .inner
                    .sd_supervisor
                    .get(plugin_uuid)
                    .ok_or_else(|| anyhow::anyhow!("SD plugin not loaded: {plugin_uuid}"))?;
                broker
                    .register_context(ActionContext {
                        context: context.clone(),
                        action_uuid: action_uuid.clone(),
                        settings: settings.clone(),
                        coordinates: (0, 0),
                    })
                    .await;
                broker.key_down(&context).await?;
                broker.key_up(&context).await?;
                Ok(())
            }
            BindingKind::Companion {
                connection_id,
                action_id,
                options,
            } => {
                let runtime = self
                    .inner
                    .comp_runtime
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("companion module engine not running"))?;
                let resp = runtime
                    .request(
                        "connection.executeAction",
                        json!({
                            "connectionId": connection_id,
                            "actionId": action_id,
                            "options": options
                        }),
                    )
                    .await?;
                if !resp.ok {
                    anyhow::bail!(resp
                        .error
                        .unwrap_or_else(|| "executeAction failed".into()));
                }
                Ok(())
            }
            BindingKind::BuiltIn { action_id, settings } => {
                self.trigger_builtin(action_id, settings, None).await
            }
            BindingKind::MultiAction { .. } => {
                anyhow::bail!("nested multi-action is not supported")
            }
        }
    }

    async fn trigger_binding_kind(&mut self, kind: &BindingKind) -> anyhow::Result<()> {
        match kind {
            BindingKind::MultiAction { steps, delay_ms } => {
                for step in steps {
                    if step.delay_before_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(
                            step.delay_before_ms as u64,
                        ))
                        .await;
                    }
                    self.execute_binding_leaf(&step.binding).await?;
                    if *delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(*delay_ms as u64))
                            .await;
                    }
                }
                Ok(())
            }
            _ => self.execute_binding_leaf(kind).await,
        }
    }

    async fn trigger_builtin(
        &mut self,
        action_id: &str,
        settings: &serde_json::Value,
        surface_id: Option<SurfaceId>,
    ) -> anyhow::Result<()> {
        match action_id {
            OPEN_FOLDER => {
                let folder: FolderSettings = serde_json::from_value(settings.clone())
                    .unwrap_or(FolderSettings { child_page_id: None });
                let child_id = folder
                    .child_page_id()
                    .ok_or_else(|| anyhow::anyhow!("folder has no child page"))?;
                if let Some(surface_id) = surface_id {
                    self.navigate_surface_to_page(surface_id, child_id, true)
                        .await?;
                } else {
                    self.set_active_page(child_id).await?;
                }
            }
            BACK_TO_PARENT => {
                if let Some(surface_id) = surface_id {
                    self.navigate_surface_back(surface_id).await?;
                } else if let Some(page_id) = self.inner.profile.active_page_id {
                    if let Some(parent) = self
                        .inner
                        .profile
                        .pages
                        .iter()
                        .find(|p| p.id == page_id)
                        .and_then(|p| p.parent_page_id)
                    {
                        self.set_active_page(parent).await?;
                    }
                }
            }
            SWITCH_PAGE => {
                let switch: SwitchPageSettings = serde_json::from_value(settings.clone())
                    .unwrap_or(SwitchPageSettings {
                        target_page_id: None,
                        target_page_name: None,
                    });
                let target = switch
                    .target_page_id
                    .as_ref()
                    .and_then(|id| uuid::Uuid::parse_str(id).ok())
                    .map(PageId)
                    .or_else(|| {
                        switch.target_page_name.as_ref().and_then(|name| {
                            self.inner
                                .profile
                                .pages
                                .iter()
                                .find(|p| p.name == *name)
                                .map(|p| p.id)
                        })
                    });
                if let Some(target) = target {
                    if let Some(surface_id) = surface_id {
                        self.navigate_surface_to_page(surface_id, target, false)
                            .await?;
                    } else {
                        self.set_active_page(target).await?;
                    }
                }
            }
            _ => anyhow::bail!("unknown built-in action: {action_id}"),
        }
        Ok(())
    }

    fn slot_id_for_context(&self, context: &str) -> Option<SlotId> {
        self.inner
            .routing
            .slot_to_context
            .iter()
            .find_map(|(slot_id, ctx)| (ctx.as_str() == context).then_some(*slot_id))
    }

    async fn apply_settings_from_context(
        &mut self,
        context: &str,
        settings: serde_json::Value,
    ) -> anyhow::Result<()> {
        let slot_id = self
            .slot_id_for_context(context)
            .ok_or_else(|| anyhow::anyhow!("no slot for context {context}"))?;
        self.update_slot_settings(slot_id, settings).await
    }

    pub async fn finalize_hid_connection(
        &mut self,
        surface_id: SurfaceId,
        orch: Arc<RwLock<Self>>,
    ) -> Result<(), String> {
        if let Some(device_key) = self.device_key_for_surface(surface_id) {
            if let Ok(Some(json)) = self.load_saved_device_preset(&device_key) {
                self.merge_device_preset(&device_key, &json, surface_id)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        self.rehydrate_profile_bindings(orch)
            .await
            .map_err(|e| e.to_string())?;
        for uuid in self.inner.sd_supervisor.plugin_uuids() {
            if let Some(broker) = self.inner.sd_supervisor.get(&uuid) {
                broker.device_did_connect().await.ok();
            }
        }
        self.render_surface_visuals(surface_id).await;
        Ok(())
    }

    async fn render_surface_visuals(&mut self, surface_id: SurfaceId) {
        self.inner.ensure_surface_page(surface_id);
        let Some(page_id) = self.inner.page_for_surface(surface_id) else {
            return;
        };
        self.refresh_page_visuals(page_id, surface_id).await;
    }

    fn clear_surface_bindings(&mut self, surface_id: SurfaceId) {
        let slot_ids: Vec<SlotId> = self
            .inner
            .profile
            .pages
            .iter()
            .flat_map(|p| p.slots.iter())
            .filter(|(_, slot)| slot.locator.surface_id == surface_id)
            .map(|(id, _)| *id)
            .collect();

        for slot_id in slot_ids {
            if let Some(context) = self.inner.routing.slot_to_context.get(&slot_id).cloned() {
                if let Some((surface_id, row, col)) =
                    self.inner.routing.context_to_cell.get(&context)
                {
                    for bridge in self.inner.bridges.values_mut() {
                        bridge
                            .context_by_cell
                            .remove(&(*surface_id, *row, *col));
                        bridge.cell_by_context.remove(&context);
                    }
                }
            }
            self.inner.routing.clear_slot(&slot_id);
        }
        self.inner.comp_bridge = None;
    }

    async fn merge_device_preset(
        &mut self,
        device_key: &str,
        json: &str,
        target_surface: SurfaceId,
    ) -> Result<(), String> {
        let preset: DevicePresetExport =
            serde_json::from_str(json).map_err(|e| format!("preset JSON invalid: {e}"))?;
        if preset.device_key != device_key {
            return Ok(());
        }
        self.clear_surface_bindings(target_surface);
        self.apply_preset_pages(&preset, target_surface).await?;
        self.save_profile().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn apply_preset_pages(
        &mut self,
        preset: &DevicePresetExport,
        target_surface: SurfaceId,
    ) -> Result<(), String> {
        for preset_page in &preset.pages {
            let page_id = PageId(
                uuid::Uuid::parse_str(&preset_page.id).map_err(|e| e.to_string())?,
            );
            if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
                page.slots
                    .retain(|_, slot| slot.locator.surface_id != target_surface);
                for (slot_id, mut slot) in preset_page.slots.clone() {
                    slot.locator.surface_id = target_surface;
                    page.slots.insert(
                        SlotId(uuid::Uuid::parse_str(&slot_id).map_err(|e| e.to_string())?),
                        slot,
                    );
                }
            } else {
                let mut slots = std::collections::HashMap::new();
                for (slot_id, mut slot) in preset_page.slots.clone() {
                    slot.locator.surface_id = target_surface;
                    slots.insert(
                        SlotId(uuid::Uuid::parse_str(&slot_id).map_err(|e| e.to_string())?),
                        slot,
                    );
                }
                self.inner.profile.pages.push(Page {
                    id: page_id,
                    name: preset_page.name.clone(),
                    slots,
                    parent_page_id: None,
                });
            }
        }
        Ok(())
    }

    pub fn profile(&self) -> &Profile {
        &self.inner.profile
    }

    pub fn profile_mut(&mut self) -> &mut Profile {
        &mut self.inner.profile
    }

    pub async fn save_profile(&self) -> anyhow::Result<()> {
        let path = paths::profiles_dir().join("default.json");
        let data = serde_json::to_string_pretty(&self.inner.profile)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub async fn register_mock_surface(&mut self, name: String) -> SurfaceId {
        let id = SurfaceId::new();
        let mock = MockSurface::new(id, name.clone());
        self.inner.surfaces.register_mock(mock).await;
        if !self.inner.profile.surfaces.iter().any(|s| s.surface_id == id) {
            self.inner.profile.surfaces.push(ideck_core::SurfaceAssignment {
                surface_id: id,
                label: name,
                emulation_profile: Some("streamdeck_mk2".into()),
            });
        }
        let _ = self.save_profile().await;
        id
    }

    pub async fn list_surface_descriptors(&self) -> Vec<ideck_surface::SurfaceDescriptor> {
        self.inner.surfaces.list().await
    }

    pub fn list_surface_drivers(&self) -> Vec<SurfaceDriverManifest> {
        self.inner.surface_drivers.list_manifests()
    }

    pub fn scan_hid_devices(&self) -> Result<Vec<HidDeviceDescriptor>, String> {
        self.scan_physical_devices()
            .map(|devices| {
                devices
                    .into_iter()
                    .filter(|d| d.driver_id == DRIVER_ID)
                    .filter_map(|d| {
                        let (kind, serial) = d.device_id.split_once(':')?;
                        Some(HidDeviceDescriptor {
                            serial: serial.to_string(),
                            kind: kind.to_string(),
                            product: d.product,
                            rows: d.rows,
                            columns: d.columns,
                            key_count: (d.rows * d.columns) as u8,
                        })
                    })
                    .collect()
            })
            .map_err(|e| e.to_string())
    }

    pub fn scan_physical_devices(&self) -> Result<Vec<DiscoveredDevice>, String> {
        self.inner
            .surface_drivers
            .scan_all()
            .map_err(|e| e.to_string())
    }

    pub async fn connect_hid_device(
        &mut self,
        serial: String,
        kind_id: String,
    ) -> Result<(ConnectHidResult, Arc<dyn PhysicalSurface>), String> {
        let device_id = format!("{kind_id}:{serial}");
        if self.hid_serials.contains_key(&serial) {
            return Err("device already connected".into());
        }

        let device_key = settings_store::device_key(&kind_id, &serial);
        let registry = settings_store::load_device_registry();
        let saved = registry.iter().find(|r| r.device_key == device_key);

        let discovered = self
            .scan_physical_devices()
            .ok()
            .and_then(|devices| {
                devices
                    .into_iter()
                    .find(|d| d.device_id == device_id)
            });
        let default_label = discovered
            .as_ref()
            .map(|d| d.label.clone())
            .unwrap_or_else(|| format!("Stream Deck ({serial})"));
        let label = saved
            .map(|r| r.label.clone())
            .unwrap_or(default_label);
        let brightness = saved.map(|r| r.brightness).unwrap_or(50);
        let (rows, columns) = discovered
            .as_ref()
            .map(|d| (d.rows, d.columns))
            .unwrap_or((3, 5));

        let surface_id = saved
            .and_then(|r| r.surface_id.as_ref())
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            .map(SurfaceId)
            .filter(|id| self.inner.profile.surfaces.iter().any(|s| s.surface_id == *id))
            .unwrap_or_else(SurfaceId::new);

        let surface = self
            .inner
            .surface_drivers
            .connect(DRIVER_ID, &device_id, surface_id, &label)
            .map_err(|e| e.to_string())?;

        if let Ok(hid) = surface.clone().as_any().downcast::<StreamDeckHidSurface>() {
            let _ = hid.set_brightness(brightness).await;
        }

        self.inner
            .surfaces
            .attach_physical(surface_id, surface.clone())
            .await;

        if !self
            .inner
            .profile
            .surfaces
            .iter()
            .any(|s| s.surface_id == surface_id)
        {
            self.inner.profile.surfaces.push(ideck_core::SurfaceAssignment {
                surface_id,
                label: label.clone(),
                emulation_profile: Some(kind_id.clone()),
            });
        } else if let Some(assignment) = self
            .inner
            .profile
            .surfaces
            .iter_mut()
            .find(|s| s.surface_id == surface_id)
        {
            assignment.label = label.clone();
            if assignment.emulation_profile.is_none() {
                assignment.emulation_profile = Some(kind_id.clone());
            }
        }
        let _ = self.save_profile().await;

        self.hid_serials.insert(serial.clone(), surface_id);
        let product = discovered
            .as_ref()
            .map(|d| d.product.clone())
            .unwrap_or_else(|| label.clone());
        self.hid_meta.insert(
            surface_id,
            HidSurfaceMeta {
                serial: serial.clone(),
                kind: kind_id.clone(),
                product: product.clone(),
            },
        );

        let _ = settings_store::upsert_device_record(DeviceRecord {
            device_key: device_key.clone(),
            serial: serial.clone(),
            kind: kind_id,
            product,
            label: label.clone(),
            brightness,
            last_seen: settings_store::now_iso(),
            surface_id: Some(surface_id.0.to_string()),
        });

        let result = ConnectHidResult {
            surface_id: surface_id.0.to_string(),
            label,
            rows,
            columns,
        };
        Ok((result, surface))
    }

    pub fn store_hid_tasks(&mut self, surface_id: SurfaceId, handles: Vec<tokio::task::JoinHandle<()>>) {
        self.hid_task_handles.insert(surface_id, handles);
    }

    pub fn scan_plugins(&self) -> anyhow::Result<ScanResult> {
        paths::ensure_dirs()?;
        let sd_roots = paths::sd_plugin_scan_roots();
        let companion_dir = paths::companion_modules_dir();
        let streamdeck = scan_sd_plugins_roots(&sd_roots)
            .into_iter()
            .map(PluginScanEntry::from_sd_plugin)
            .collect();
        let companion = scan_module_dirs(&companion_dir)
            .into_iter()
            .map(PluginScanEntry::from_companion_dir)
            .collect();
        Ok(ScanResult {
            streamdeck,
            companion,
            streamdeck_dirs: sd_roots
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            companion_dir: companion_dir.display().to_string(),
        })
    }

    async fn primary_capabilities(&self) -> SurfaceCapabilities {
        let descriptors = self.inner.surfaces.list().await;
        descriptors
            .first()
            .map(|d| d.capabilities.clone())
            .unwrap_or_else(SurfaceCapabilities::streamdeck_mk2)
    }

    fn devices_json(caps: &SurfaceCapabilities, device_id: &str) -> serde_json::Value {
        json!([{
            "id": device_id,
            "name": "integratedeck",
            "type": 0,
            "size": { "columns": caps.columns, "rows": caps.rows }
        }])
    }

    pub async fn load_sd_plugin(
        &mut self,
        plugin_path: PathBuf,
        orch: Arc<RwLock<Orchestrator>>,
    ) -> anyhow::Result<LoadPluginResult> {
        let manifest = StreamDeckManifest::load(&plugin_path)?;

        if self.inner.sd_supervisor.get(&manifest.uuid).is_some() {
            let meta = self
                .inner
                .sd_supervisor
                .meta(&manifest.uuid)
                .expect("meta exists");
            return Ok(LoadPluginResult {
                plugin_uuid: manifest.uuid,
                port: meta.port,
                name: manifest.name,
            });
        }

        let caps = self.primary_capabilities().await;
        let device_id = format!("integratedeck-virtual-{}", manifest.uuid.replace('.', "-"));
        let devices = Self::devices_json(&caps, &device_id);

        let (broker, events) = StreamDeckBroker::start(&plugin_path, devices).await?;
        let port = broker.port();

        let bridge = SdSurfaceBridge::new(manifest.uuid.clone(), caps);
        self.inner
            .bridges
            .insert(manifest.uuid.clone(), bridge);

        let meta = LoadedSdPlugin {
            path: plugin_path,
            name: manifest.name.clone(),
            plugin_uuid: manifest.uuid.clone(),
            port,
        };
        let plugin_uuid = manifest.uuid.clone();
        let result = LoadPluginResult {
            plugin_uuid: manifest.uuid.clone(),
            port,
            name: manifest.name,
        };

        let drain = PluginSupervisor::start_drain(events, move |ev| {
            let orch = orch.clone();
            let uuid = plugin_uuid.clone();
            async move {
                let mut o = orch.write().await;
                o.apply_broker_event(&uuid, ev).await.ok();
            }
        });
        self.inner
            .sd_supervisor
            .insert(meta, broker, drain);

        Ok(result)
    }

    pub async fn unload_sd_plugin(&mut self, plugin_uuid: Option<String>) -> Result<(), String> {
        match plugin_uuid {
            Some(uuid) => {
                self.inner.sd_supervisor.unload(&uuid).await;
                self.inner.bridges.remove(&uuid);
            }
            None => {
                self.inner.sd_supervisor.unload_all().await;
                self.inner.bridges.clear();
            }
        }
        Ok(())
    }

    pub async fn disconnect_hid_device(&mut self, surface_id: SurfaceId) -> Result<(), String> {
        if !self.hid_meta.contains_key(&surface_id) {
            return Err("指定の物理デバイスは接続されていません。".into());
        }
        if let Some(handles) = self.hid_task_handles.remove(&surface_id) {
            for handle in handles {
                handle.abort();
            }
        }
        let meta = self.hid_meta.remove(&surface_id).ok_or("missing device meta")?;
        self.hid_serials.retain(|_, id| *id != surface_id);
        self.inner.surfaces.unregister(surface_id).await;

        let device_key = settings_store::device_key(&meta.kind, &meta.serial);
        let mut records = settings_store::load_device_registry();
        if let Some(record) = records.iter_mut().find(|r| r.device_key == device_key) {
            record.last_seen = settings_store::now_iso();
        }
        let _ = settings_store::save_device_registry(&records);

        // Re-register as mock so the surface remains available in the main UI offline.
        if self
            .inner
            .profile
            .surfaces
            .iter()
            .any(|s| s.surface_id == surface_id)
        {
            let label = self
                .inner
                .profile
                .surfaces
                .iter()
                .find(|s| s.surface_id == surface_id)
                .map(|s| s.label.clone())
                .unwrap_or_else(|| meta.product.clone());
            let mock = MockSurface::with_id(surface_id, label);
            self.inner.surfaces.register_mock(mock).await;
        }

        HubEvents::emit_surfaces_changed(&self.app, None);
        Ok(())
    }

    pub fn global_settings(&self) -> AppGlobalSettings {
        settings_store::load_global_settings()
    }

    pub fn save_global_settings(&self, settings: &AppGlobalSettings) -> Result<(), String> {
        settings_store::save_global_settings(settings)
    }

    pub async fn list_settings_devices(&self) -> Vec<SettingsDeviceEntry> {
        let records = self.sync_device_registry().unwrap_or_else(|e| {
            warn!("sync_device_registry failed: {e}");
            settings_store::load_device_registry()
        });
        let scanned = self.scan_hid_devices().unwrap_or_default();

        records
            .into_iter()
            .map(|record| {
                let connected_surface = self.hid_serials.get(&record.serial).copied();
                let is_scanned = scanned
                    .iter()
                    .any(|d| d.serial == record.serial && d.kind == record.kind);
                let status = if connected_surface.is_some() {
                    "connected"
                } else if is_scanned {
                    "available"
                } else {
                    "offline"
                };
                let saved_surface_id = record.surface_id.clone();
                let surface_id = connected_surface
                    .map(|id| id.0.to_string())
                    .or(saved_surface_id);
                let (rows, columns) = scanned
                    .iter()
                    .find(|d| d.serial == record.serial && d.kind == record.kind)
                    .map(|d| (d.rows, d.columns))
                    .unwrap_or((3, 5));
                let label = self
                    .inner
                    .profile
                    .surfaces
                    .iter()
                    .find(|s| {
                        record
                            .surface_id
                            .as_ref()
                            .is_some_and(|id| id == &s.surface_id.0.to_string())
                    })
                    .map(|s| s.label.clone())
                    .unwrap_or(record.label);
                SettingsDeviceEntry {
                    device_key: record.device_key,
                    serial: record.serial,
                    kind: record.kind,
                    product: record.product,
                    label,
                    status: status.into(),
                    surface_id,
                    firmware_version: None,
                    brightness: record.brightness,
                    rows,
                    columns,
                }
            })
            .collect()
    }

    pub async fn get_settings_device(&self, device_key: &str) -> Option<SettingsDeviceEntry> {
        let mut entry = self
            .list_settings_devices()
            .await
            .into_iter()
            .find(|d| d.device_key == device_key)?;
        if entry.status == "connected" {
            if let Some(surface_id) = entry.surface_id.as_ref() {
                if let Ok(id) = uuid::Uuid::parse_str(surface_id) {
                    let surface_id = SurfaceId(id);
                    if let Some(physical) = self.inner.surfaces.physical(surface_id) {
                        if let Ok(hid) = physical.as_any().downcast::<StreamDeckHidSurface>() {
                            entry.firmware_version = hid.firmware_version().await.ok();
                        }
                    }
                }
            }
        }
        Some(entry)
    }

    pub async fn set_device_label(&mut self, device_key: &str, label: String) -> Result<(), String> {
        let label = label.trim();
        if label.is_empty() {
            return Err("ラベル名を入力してください。".into());
        }
        let surface_id = {
            let mut records = settings_store::load_device_registry();
            let record = records
                .iter_mut()
                .find(|r| r.device_key == device_key)
                .ok_or("デバイスが見つかりません。")?;
            record.label = label.to_string();
            let surface_id = record.surface_id.clone();
            settings_store::save_device_registry(&records)?;
            surface_id
        };

        if let Some(surface_id) = surface_id {
            let surface_id = SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
            if let Some(assignment) = self
                .inner
                .profile
                .surfaces
                .iter_mut()
                .find(|s| s.surface_id == surface_id)
            {
                assignment.label = label.to_string();
            }
            let _ = self.save_profile().await;
        }
        Ok(())
    }

    pub async fn set_device_brightness(
        &mut self,
        device_key: &str,
        brightness: u8,
    ) -> Result<(), String> {
        let brightness = brightness.min(100);
        let surface_id = {
            let mut records = settings_store::load_device_registry();
            let record = records
                .iter_mut()
                .find(|r| r.device_key == device_key)
                .ok_or("デバイスが見つかりません。")?;
            record.brightness = brightness;
            let surface_id = record.surface_id.clone();
            settings_store::save_device_registry(&records)?;
            surface_id
        };

        if let Some(surface_id) = surface_id {
            let surface_id = SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
            if let Some(physical) = self.inner.surfaces.physical(surface_id) {
                if let Ok(hid) = physical.as_any().downcast::<StreamDeckHidSurface>() {
                    hid.set_brightness(brightness).await?;
                }
            }
        }
        Ok(())
    }

    pub async fn connect_settings_device(
        &mut self,
        device_key: &str,
    ) -> Result<(ConnectHidResult, Arc<dyn PhysicalSurface>), String> {
        let records = settings_store::load_device_registry();
        let record = records
            .iter()
            .find(|r| r.device_key == device_key)
            .ok_or("デバイスが見つかりません。")?;
        self.connect_hid_device(record.serial.clone(), record.kind.clone())
            .await
    }

    pub fn export_device_preset(&self, device_key: &str) -> Result<String, String> {
        let records = settings_store::load_device_registry();
        let record = records
            .iter()
            .find(|r| r.device_key == device_key)
            .ok_or("デバイスが見つかりません。")?;
        let surface_id = record
            .surface_id
            .as_ref()
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            .map(SurfaceId);
        let surface_id = surface_id.or_else(|| {
            self.inner.profile.surfaces.first().map(|s| s.surface_id)
        }).ok_or("このデバイスに関連付けられたサーフェスがありません。")?;

        let preset = DevicePresetExport {
            device_key: device_key.to_string(),
            label: record.label.clone(),
            surface_id: surface_id.0.to_string(),
            profile_name: self.inner.profile.name.clone(),
            pages: self
                .inner
                .profile
                .pages
                .iter()
                .map(|page| DevicePresetPage {
                    id: page.id.0.to_string(),
                    name: page.name.clone(),
                    slots: page
                        .slots
                        .iter()
                        .filter(|(_, slot)| slot.locator.surface_id == surface_id)
                        .map(|(id, slot)| (id.0.to_string(), slot.clone()))
                        .collect(),
                })
                .collect(),
        };
        let json = serde_json::to_string_pretty(&preset).map_err(|e| e.to_string())?;
        paths::ensure_dirs().map_err(|e| e.to_string())?;
        std::fs::write(paths::device_preset_file(device_key), &json).map_err(|e| e.to_string())?;
        Ok(json)
    }

    pub async fn import_device_preset(
        &mut self,
        device_key: &str,
        json: &str,
        orch: Arc<RwLock<Self>>,
    ) -> Result<(), String> {
        let preset: DevicePresetExport =
            serde_json::from_str(json).map_err(|e| format!("プリセット JSON が不正です: {e}"))?;
        if preset.device_key != device_key {
            return Err("選択中のデバイスとプリセットのデバイスが一致しません。".into());
        }

        let target_surface = self.resolve_preset_surface_id(device_key, &preset)?;
        self.clear_surface_bindings(target_surface);
        self.apply_preset_pages(&preset, target_surface)
            .await
            .map_err(|e| e.to_string())?;
        self.save_profile().await.map_err(|e| e.to_string())?;
        paths::ensure_dirs().map_err(|e| e.to_string())?;
        std::fs::write(paths::device_preset_file(device_key), json).map_err(|e| e.to_string())?;
        self.rehydrate_profile_bindings(orch.clone())
            .await
            .map_err(|e| e.to_string())?;
        self.render_surface_visuals(target_surface).await;
        Ok(())
    }

    pub fn load_saved_device_preset(&self, device_key: &str) -> Result<Option<String>, String> {
        let path = paths::device_preset_file(device_key);
        if !path.exists() {
            return Ok(None);
        }
        std::fs::read_to_string(path)
            .map(Some)
            .map_err(|e| e.to_string())
    }

    fn resolve_preset_surface_id(
        &self,
        device_key: &str,
        preset: &DevicePresetExport,
    ) -> Result<SurfaceId, String> {
        let records = settings_store::load_device_registry();
        if let Some(record) = records.iter().find(|r| r.device_key == device_key) {
            if let Some(id) = record.surface_id.as_ref() {
                return Ok(SurfaceId(
                    uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?,
                ));
            }
        }
        Ok(SurfaceId(
            uuid::Uuid::parse_str(&preset.surface_id).map_err(|e| e.to_string())?,
        ))
    }

    pub async fn runtime_status(&self) -> anyhow::Result<RuntimeStatus> {
        let scan = self.scan_plugins()?;
        let loaded = self.inner.sd_supervisor.list();

        let plugins: Vec<PluginRuntimeEntry> = scan
            .streamdeck
            .into_iter()
            .map(|entry| {
                let loaded_meta = loaded.iter().find(|p| p.path.display().to_string() == entry.path);
                let running = loaded_meta.is_some();
                PluginRuntimeEntry {
                    status: if running { "running" } else { "stopped" }.into(),
                    path: entry.path,
                    name: entry.name,
                    bundle_name: entry.bundle_name,
                    uuid: entry.uuid,
                    version: entry.version,
                    port: loaded_meta.map(|p| p.port),
                    pi_url: loaded_meta.map(|p| format!("ws://127.0.0.1:{}", p.port)),
                }
            })
            .collect();

        let descriptors = self.list_surface_descriptors().await;
        let mut surfaces = Vec::new();
        for desc in descriptors {
            let backend = match &desc.backend {
                ideck_surface::SurfaceBackend::Mock => "mock".into(),
                ideck_surface::SurfaceBackend::Physical { driver_id, .. } => driver_id.clone(),
            };
            let (serial, kind, product) = self
                .hid_meta
                .get(&desc.id)
                .map(|m| (Some(m.serial.clone()), Some(m.kind.clone()), Some(m.product.clone())))
                .unwrap_or((None, None, None));
            let status = match &desc.backend {
                ideck_surface::SurfaceBackend::Mock => "connected",
                ideck_surface::SurfaceBackend::Physical { .. } if self.hid_meta.contains_key(&desc.id) => {
                    "connected"
                }
                _ => "disconnected",
            };
            surfaces.push(SurfaceRuntimeEntry {
                surface_id: desc.id.0.to_string(),
                label: desc.name,
                backend: backend.into(),
                status: status.into(),
                rows: desc.capabilities.rows,
                columns: desc.capabilities.columns,
                serial,
                kind,
                product,
            });
        }

        let usb_devices: Vec<UsbDeviceRuntimeEntry> = self
            .scan_hid_devices()
            .unwrap_or_default()
            .into_iter()
            .map(|d| {
                let connected_surface = self.hid_serials.get(&d.serial).copied();
                UsbDeviceRuntimeEntry {
                    serial: d.serial,
                    kind: d.kind,
                    product: d.product,
                    rows: d.rows,
                    columns: d.columns,
                    status: if connected_surface.is_some() {
                        "connected".into()
                    } else {
                        "available".into()
                    },
                    surface_id: connected_surface.map(|id| id.0.to_string()),
                }
            })
            .collect();

        let sd_plugins: Vec<SdPluginRuntime> = loaded
            .iter()
            .map(|p| SdPluginRuntime {
                status: "running".into(),
                path: p.path.display().to_string(),
                name: p.name.clone(),
                plugin_uuid: p.plugin_uuid.clone(),
                port: p.port,
                pi_url: format!("ws://127.0.0.1:{}", p.port),
            })
            .collect();

        Ok(RuntimeStatus {
            sd_plugins,
            plugins,
            surfaces,
            usb_devices,
            streamdeck_dirs: scan.streamdeck_dirs,
            companion: CompanionRuntimeStatus {
                companion_host_running: self.inner.comp_runtime.is_some(),
                connections: self.connections(),
                modules: scan.companion,
            },
        })
    }

    pub fn loaded_plugin_dir(&self, plugin_uuid: &str) -> Option<&PathBuf> {
        self.inner
            .sd_supervisor
            .meta(plugin_uuid)
            .map(|p| &p.path)
    }

    pub async fn bind_slot_streamdeck(
        &mut self,
        slot_id: SlotId,
        plugin_uuid: String,
        action_uuid: String,
    ) -> anyhow::Result<String> {
        let instance_id = ActionInstanceId::new();
        let context =
            SdSurfaceBridge::build_context_id(&plugin_uuid, &action_uuid, &instance_id.0);

        let page_id = self
            .inner
            .profile
            .active_page_id
            .ok_or_else(|| anyhow::anyhow!("no active page"))?;

        let (row, col, surface_id) = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            let slot = page
                .slots
                .get(&slot_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
            (
                slot.locator.row,
                slot.locator.column,
                slot.locator.surface_id,
            )
        };

        if let Some(bridge) = self.inner.bridges.get_mut(&plugin_uuid) {
            bridge.register_cell(surface_id, row, col, context.clone());
        }
        self.inner
            .routing
            .register_sd_cell(surface_id, row, col, context.clone());
        self.inner
            .routing
            .register_sd_slot(slot_id, context.clone());

        let broker = self
            .inner
            .sd_supervisor
            .get(&plugin_uuid)
            .ok_or_else(|| anyhow::anyhow!("SD plugin not loaded: {plugin_uuid}"))?;

        broker
            .register_context(ActionContext {
                context: context.clone(),
                action_uuid: action_uuid.clone(),
                settings: serde_json::json!({}),
                coordinates: (col, row),
            })
            .await;
        broker.will_appear(&context).await?;

        if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
            if let Some(slot) = page.slots.get_mut(&slot_id) {
                slot.binding = Some(Binding::stream_deck(
                    plugin_uuid,
                    action_uuid,
                    instance_id,
                    serde_json::json!({}),
                ));
            }
        }

        self.persist_slot_changes(surface_id).await?;
        Ok(context)
    }

    pub async fn bind_slot_companion(
        &mut self,
        slot_id: SlotId,
        connection_id: String,
        action_id: String,
        options: serde_json::Value,
    ) -> anyhow::Result<()> {
        let page_id = self
            .inner
            .profile
            .active_page_id
            .ok_or_else(|| anyhow::anyhow!("no active page"))?;

        self.inner.routing.register_companion_slot(
            slot_id,
            connection_id.clone(),
            action_id.clone(),
        );

        if self.inner.comp_bridge.is_none() {
            self.inner.comp_bridge = Some(CompSurfaceBridge::new());
        }
        if let Some(bridge) = self.inner.comp_bridge.as_mut() {
            let (row, col) = {
                let page = self
                    .inner
                    .profile
                    .pages
                    .iter()
                    .find(|p| p.id == page_id)
                    .ok_or_else(|| anyhow::anyhow!("page not found"))?;
                let slot = page
                    .slots
                    .get(&slot_id)
                    .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
                (slot.locator.row, slot.locator.column)
            };
            let control_id = format!("{connection_id}:{action_id}");
            bridge.register_cell(row, col, control_id);
        }

        if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
            if let Some(slot) = page.slots.get_mut(&slot_id) {
                slot.binding = Some(Binding::companion(
                    connection_id,
                    action_id,
                    options,
                ));
            }
        }

        let surface_id = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            page.slots
                .get(&slot_id)
                .map(|s| s.locator.surface_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?
        };
        self.persist_slot_changes(surface_id).await?;
        Ok(())
    }

    pub async fn unbind_slot(&mut self, slot_id: SlotId) -> anyhow::Result<()> {
        let page_id = self
            .inner
            .profile
            .active_page_id
            .ok_or_else(|| anyhow::anyhow!("no active page"))?;

        let binding = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            page.slots.get(&slot_id).and_then(|s| s.binding.clone())
        };

        let surface_id = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            page.slots
                .get(&slot_id)
                .map(|s| s.locator.surface_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?
        };

        let context = self.inner.routing.slot_to_context.get(&slot_id).cloned();
        let plugin_uuid = binding.as_ref().and_then(|b| match &b.kind {
            BindingKind::StreamDeck { plugin_uuid, .. } => Some(plugin_uuid.clone()),
            _ => None,
        });

        if let Some(ctx) = context {
            if let Some(uuid) = &plugin_uuid {
                if let Some(broker) = self.inner.sd_supervisor.get(uuid) {
                    broker.will_disappear(&ctx).await.ok();
                }
            }
            if let Some((surface_id, row, col)) = self.inner.routing.context_to_cell.get(&ctx) {
                if let Some(uuid) = &plugin_uuid {
                    if let Some(bridge) = self.inner.bridges.get_mut(uuid) {
                        bridge
                            .context_by_cell
                            .remove(&(*surface_id, *row, *col));
                        bridge.cell_by_context.remove(&ctx);
                    }
                }
            }
        }

        self.inner.routing.clear_slot(&slot_id);

        if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
            if let Some(slot) = page.slots.get_mut(&slot_id) {
                slot.binding = None;
            }
        }

        self.persist_slot_changes(surface_id).await?;
        Ok(())
    }

    pub async fn apply_slot_snapshot(
        &mut self,
        slot_id: SlotId,
        binding: Option<serde_json::Value>,
        appearance: SlotAppearance,
        orch: Arc<RwLock<Self>>,
    ) -> anyhow::Result<()> {
        self.unbind_slot(slot_id).await?;

        let page_id = self
            .page_id_for_slot(slot_id)
            .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
        let (surface_id, row, col) = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            let slot = page
                .slots
                .get(&slot_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
            (
                slot.locator.surface_id,
                slot.locator.row,
                slot.locator.column,
            )
        };

        if let Some(binding_value) = binding {
            let kind = fresh_binding_kind(&parse_binding_kind(binding_value)?);
            match &kind {
                BindingKind::StreamDeck {
                    plugin_uuid,
                    action_uuid,
                    instance_id,
                    settings,
                } => {
                    if self.inner.sd_supervisor.get(plugin_uuid).is_none() {
                        if let Some(path) = self.find_sd_plugin_path(plugin_uuid) {
                            self.load_sd_plugin(path, orch).await?;
                        } else {
                            anyhow::bail!("SD plugin not loaded: {plugin_uuid}");
                        }
                    }
                    self.restore_sd_binding(
                        slot_id,
                        plugin_uuid,
                        action_uuid,
                        instance_id.clone(),
                        settings.clone(),
                        surface_id,
                        row,
                        col,
                    )
                    .await?;
                }
                BindingKind::Companion {
                    connection_id,
                    action_id,
                    ..
                } => {
                    self.restore_companion_binding(slot_id, connection_id, action_id, row, col);
                }
                BindingKind::BuiltIn { .. } | BindingKind::MultiAction { .. } => {}
            }

            if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
                if let Some(slot) = page.slots.get_mut(&slot_id) {
                    slot.binding = Some(Binding { kind });
                }
            }
        }

        if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
            if let Some(slot) = page.slots.get_mut(&slot_id) {
                slot.appearance = appearance;
            }
        }

        self.persist_slot_changes(surface_id).await?;
        self.refresh_page_visuals(page_id, surface_id).await;
        Ok(())
    }

    pub async fn update_slot_settings(
        &mut self,
        slot_id: SlotId,
        settings: serde_json::Value,
    ) -> anyhow::Result<()> {
        let page_id = self
            .page_id_for_slot(slot_id)
            .ok_or_else(|| anyhow::anyhow!("slot not found"))?;

        let (binding_kind, context, coordinates) = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            let slot = page
                .slots
                .get(&slot_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
            (
                slot.binding.clone(),
                self.inner.routing.slot_to_context.get(&slot_id).cloned(),
                (slot.locator.column, slot.locator.row),
            )
        };

        match binding_kind {
            Some(Binding {
                kind:
                    BindingKind::StreamDeck {
                        plugin_uuid,
                        action_uuid,
                        ..
                    },
            }) => {
                if let Some(ctx) = context {
                    let broker = self
                        .inner
                        .sd_supervisor
                        .get(&plugin_uuid)
                        .ok_or_else(|| anyhow::anyhow!("SD plugin not loaded: {plugin_uuid}"))?;
                    broker
                        .register_context(ActionContext {
                            context: ctx,
                            action_uuid,
                            settings: settings.clone(),
                            coordinates,
                        })
                        .await;
                }
                if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
                    if let Some(slot) = page.slots.get_mut(&slot_id) {
                        if let Some(Binding {
                            kind: BindingKind::StreamDeck { settings: s, .. },
                        }) = &mut slot.binding
                        {
                            *s = settings.clone();
                        }
                    }
                }
            }
            Some(Binding {
                kind: BindingKind::Companion { .. },
            }) => {
                if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
                    if let Some(slot) = page.slots.get_mut(&slot_id) {
                        if let Some(Binding {
                            kind: BindingKind::Companion { options, .. },
                        }) = &mut slot.binding
                        {
                            *options = settings;
                        }
                    }
                }
            }
            None => anyhow::bail!("slot has no binding"),
            Some(Binding {
                kind: BindingKind::BuiltIn { .. },
            }) => {
                if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
                    if let Some(slot) = page.slots.get_mut(&slot_id) {
                        if let Some(Binding {
                            kind: BindingKind::BuiltIn { settings: s, .. },
                        }) = &mut slot.binding
                        {
                            *s = settings;
                        }
                    }
                }
            }
            Some(Binding {
                kind: BindingKind::MultiAction { .. },
            }) => {}
        }

        let surface_id = {
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            page.slots
                .get(&slot_id)
                .map(|s| s.locator.surface_id)
                .ok_or_else(|| anyhow::anyhow!("slot not found"))?
        };
        self.persist_slot_changes(surface_id).await?;
        Ok(())
    }

    pub async fn list_plugin_actions(
        &self,
        connection_id: Option<String>,
        plugin_uuid: Option<String>,
    ) -> anyhow::Result<Vec<PluginActionInfo>> {
        if let Some(connection_id) = connection_id {
            let runtime = self
                .inner
                .comp_runtime
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("companion module engine not running"))?;
            let resp = runtime
                .request(
                    "connection.getDefinitions",
                    json!({ "connectionId": connection_id }),
                )
                .await?;
            if !resp.ok {
                anyhow::bail!(resp.error.unwrap_or_else(|| "getDefinitions failed".into()));
            }
            let actions = resp
                .result
                .as_ref()
                .and_then(|r| r.get("actions"))
                .and_then(|a| a.as_array())
                .cloned()
                .unwrap_or_default();
            return Ok(actions
                .into_iter()
                .filter_map(|a| {
                    let id = a.get("id")?.as_str()?.to_string();
                    let name = a
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&id)
                        .to_string();
                    Some(PluginActionInfo {
                        id,
                        name,
                        source: "companion".into(),
                    })
                })
                .collect());
        }

        if let Some(plugin_uuid) = plugin_uuid {
            if let Some(meta) = self.inner.sd_supervisor.meta(&plugin_uuid) {
                let actions = StreamDeckManifest::list_actions(&meta.path)?;
                return Ok(sd_manifest_actions(actions));
            }
            let scan = self.scan_plugins()?;
            if let Some(entry) = scan
                .streamdeck
                .into_iter()
                .find(|e| {
                    e.uuid.as_deref() == Some(plugin_uuid.as_str())
                        || effective_plugin_uuid(
                            std::path::Path::new(&e.path),
                            e.uuid.as_deref(),
                        ) == plugin_uuid
                })
            {
                let actions =
                    StreamDeckManifest::list_actions(std::path::Path::new(&entry.path))?;
                return Ok(sd_manifest_actions(actions));
            }
            anyhow::bail!("SD plugin not found: {plugin_uuid}");
        }

        Ok(Vec::new())
    }

    pub async fn set_active_page(&mut self, page_id: PageId) -> anyhow::Result<()> {
        if !self.inner.profile.pages.iter().any(|p| p.id == page_id) {
            anyhow::bail!("page not found");
        }
        self.inner.profile.active_page_id = Some(page_id);
        self.save_profile().await?;
        for surface in self.inner.profile.surfaces.clone() {
            self.inner.surface_pages.insert(surface.surface_id, page_id);
            self.sync_sd_routing_for_surface_page(surface.surface_id, page_id)
                .await;
            self.refresh_page_visuals(page_id, surface.surface_id).await;
        }
        Ok(())
    }

    pub async fn delete_slot(&mut self, slot_id: SlotId) -> anyhow::Result<()> {
        self.unbind_slot(slot_id).await?;
        let page_id = self
            .inner
            .profile
            .active_page_id
            .ok_or_else(|| anyhow::anyhow!("no active page"))?;
        if let Some(page) = self.inner.profile.pages.iter_mut().find(|p| p.id == page_id) {
            page.slots.remove(&slot_id);
        }
        Ok(())
    }

    pub async fn trigger_slot(&mut self, slot_id: SlotId) -> anyhow::Result<()> {
        let binding = {
            let page_id = self
                .inner
                .profile
                .active_page_id
                .ok_or_else(|| anyhow::anyhow!("no active page"))?;
            let page = self
                .inner
                .profile
                .pages
                .iter()
                .find(|p| p.id == page_id)
                .ok_or_else(|| anyhow::anyhow!("page not found"))?;
            page.slots.get(&slot_id).and_then(|s| s.binding.clone())
        };

        let Some(binding) = binding else {
            return Ok(());
        };

        match &binding.kind {
            BindingKind::StreamDeck { plugin_uuid, .. } => {
                let context = self
                    .inner
                    .routing
                    .slot_to_context
                    .get(&slot_id)
                    .cloned()
                    .or_else(|| {
                        self.inner.bridges.values().find_map(|b| {
                            let page_id = self.inner.profile.active_page_id?;
                            let page = self.inner.profile.pages.iter().find(|p| p.id == page_id)?;
                            let slot = page.slots.get(&slot_id)?;
                            b.context_by_cell
                                .get(&(
                                    slot.locator.surface_id,
                                    slot.locator.row,
                                    slot.locator.column,
                                ))
                                .cloned()
                        })
                    })
                    .ok_or_else(|| anyhow::anyhow!("no context for slot"))?;

                let broker = self
                    .inner
                    .sd_supervisor
                    .get(plugin_uuid)
                    .ok_or_else(|| anyhow::anyhow!("SD plugin not loaded: {plugin_uuid}"))?;
                broker.key_down(&context).await?;
                broker.key_up(&context).await?;
            }
            _ => {
                self.trigger_binding_kind(&binding.kind).await?;
            }
        }
        Ok(())
    }

    pub async fn apply_surface_input(
        &mut self,
        surface_id: SurfaceId,
        input: SurfaceInput,
    ) -> anyhow::Result<()> {
        self.inner.device_bus.publish(surface_id, input.clone());

        for (plugin_uuid, bridge) in &self.inner.bridges {
            if let Some(broker) = self.inner.sd_supervisor.get(plugin_uuid) {
                match bridge.surface_input_to_sd_event(surface_id, &input) {
                    Some((context, "keyDown")) => {
                        if let Err(e) = broker.key_down(&context).await {
                            warn!("SD keyDown ({context}): {e}");
                        }
                    }
                    Some((context, "keyUp")) => {
                        if let Err(e) = broker.key_up(&context).await {
                            warn!("SD keyUp ({context}): {e}");
                        }
                    }
                    Some((context, "dialRotate")) => {
                        if let SurfaceInput::EncoderRotate { ticks, pressed, .. } = &input {
                            if let Err(e) = broker.dial_rotate(&context, *ticks, *pressed).await {
                                warn!("SD dialRotate ({context}): {e}");
                            }
                        }
                    }
                    Some((context, "dialPress")) => {
                        if broker.key_down(&context).await.is_ok() {
                            let _ = broker.key_up(&context).await;
                        }
                    }
                    _ => {}
                }
            }
        }

        if let SurfaceInput::KeyDown { address } = &input {
            self.inner.ensure_surface_page(surface_id);
            let page_id = self
                .inner
                .page_for_surface(surface_id)
                .or(self.inner.profile.active_page_id);
            let mut to_trigger: Vec<BindingKind> = Vec::new();
            if let Some(page_id) = page_id {
                if let Some(page) = self.inner.profile.pages.iter().find(|p| p.id == page_id) {
                    for slot in page.slots.values() {
                        if slot.locator.surface_id != surface_id {
                            continue;
                        }
                        if slot.locator.row != address.row
                            || slot.locator.column != address.column
                        {
                            continue;
                        }
                        if let Some(binding) = &slot.binding {
                            match &binding.kind {
                                BindingKind::StreamDeck { .. } => {}
                                other => to_trigger.push(other.clone()),
                            }
                        }
                    }
                }
            }
            for kind in to_trigger {
                match &kind {
                    BindingKind::BuiltIn { action_id, settings } => {
                        self.trigger_builtin(action_id, settings, Some(surface_id))
                            .await?;
                    }
                    _ => {
                        self.trigger_binding_kind(&kind).await?;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn apply_broker_event(
        &mut self,
        plugin_uuid: &str,
        ev: BrokerEvent,
    ) -> anyhow::Result<()> {
        match &ev {
            BrokerEvent::PluginRegistered { .. } => {
                if let Some(broker) = self.inner.sd_supervisor.get(plugin_uuid) {
                    broker.device_did_connect().await.ok();
                }
            }
            BrokerEvent::ShowAlert { context } => {
                if let Some((surface_id, row, col)) =
                    self.cell_location_for_context(plugin_uuid, context)
                {
                    self.flash_cell_feedback(surface_id, row, col, (255, 48, 48))
                        .await;
                    HubEvents::emit_plugin_status(
                        &self.app,
                        PluginStatusPayload {
                            plugin_uuid: plugin_uuid.to_string(),
                            status: "alert".into(),
                            message: Some(format!("alert on {context}")),
                            row: Some(row),
                            column: Some(col),
                        },
                    );
                }
            }
            BrokerEvent::ShowOk { context } => {
                if let Some((surface_id, row, col)) =
                    self.cell_location_for_context(plugin_uuid, context)
                {
                    self.flash_cell_feedback(surface_id, row, col, (48, 200, 72))
                        .await;
                    HubEvents::emit_plugin_status(
                        &self.app,
                        PluginStatusPayload {
                            plugin_uuid: plugin_uuid.to_string(),
                            status: "ok".into(),
                            message: Some(format!("ok on {context}")),
                            row: Some(row),
                            column: Some(col),
                        },
                    );
                }
            }
            BrokerEvent::SendToPropertyInspector { context, payload } => {
                HubEvents::emit_pi_message(&self.app, context, payload.clone());
            }
            BrokerEvent::SettingsChanged { context, settings } => {
                if let Err(e) = self
                    .apply_settings_from_context(context, settings.clone())
                    .await
                {
                    tracing::warn!("persist PI settings failed: {e}");
                }
                HubEvents::emit_settings(
                    &self.app,
                    SettingsChangedPayload {
                        context: context.clone(),
                        settings: settings.clone(),
                    },
                );
            }
            _ => {}
        }

        if let Some(bridge) = self.inner.bridges.get(plugin_uuid) {
            let replace_image = matches!(
                ev,
                BrokerEvent::SetImage {
                    image: ImageSetResult::Set(_) | ImageSetResult::Cleared,
                    ..
                }
            );
            let replace_title = matches!(ev, BrokerEvent::SetTitle { .. });
            let update_state =
                matches!(ev, BrokerEvent::SetImage { .. } | BrokerEvent::SetState { .. });
            let mut scratch: std::collections::HashMap<(u32, u32), VisualState> =
                std::collections::HashMap::new();
            let updates = bridge.broker_event_to_updates(&ev, &mut scratch);
            for update in updates {
                let context = match &ev {
                    BrokerEvent::SetImage { context, .. }
                    | BrokerEvent::SetTitle { context, .. }
                    | BrokerEvent::SetState { context, .. } => Some(context.as_str()),
                    _ => None,
                };
                let (surface_id, page_id) = if let Some(ctx) = context {
                    let loc = self.cell_location_for_context(plugin_uuid, ctx);
                    let page_id = self
                        .page_id_for_context(ctx)
                        .or(self.inner.profile.active_page_id)
                        .unwrap_or_else(PageId::new);
                    (
                        loc.map(|(s, _, _)| s).unwrap_or_else(|| {
                            self.surface_id_for_cell(update.address.row, update.address.column)
                        }),
                        page_id,
                    )
                } else {
                    (
                        self.surface_id_for_cell(update.address.row, update.address.column),
                        self.inner
                            .profile
                            .active_page_id
                            .unwrap_or_else(PageId::new),
                    )
                };
                let cell_key = (page_id, update.address.row, update.address.column);
                let merged = {
                    let existing = self
                        .inner
                        .cell_visuals
                        .get(&cell_key)
                        .cloned()
                        .unwrap_or_default();
                    merge_visual(existing, &update.visual, update_state, replace_image, replace_title)
                };
                self.inner.cell_visuals.insert(cell_key, merged.clone());
                self.push_cell_render(
                    surface_id,
                    update.address.row,
                    update.address.column,
                    merged,
                )
                .await;
            }
        }
        Ok(())
    }

    fn cell_location_for_context(
        &self,
        plugin_uuid: &str,
        context: &str,
    ) -> Option<(SurfaceId, u32, u32)> {
        self.inner
            .bridges
            .get(plugin_uuid)?
            .cell_by_context
            .get(context)
            .copied()
    }

    async fn push_cell_render(
        &self,
        surface_id: SurfaceId,
        row: u32,
        column: u32,
        visual: VisualState,
    ) {
        let update = CellUpdate {
            address: CellAddress { row, column },
            visual: visual.clone(),
        };
        self.inner
            .surfaces
            .render(surface_id, &[update])
            .await
            .ok();
        HubEvents::emit_visual(
            &self.app,
            VisualUpdatedPayload {
                row,
                column,
                visual,
            },
        );
    }

    async fn flash_cell_feedback(
        &mut self,
        surface_id: SurfaceId,
        row: u32,
        col: u32,
        rgb: (u8, u8, u8),
    ) {
        let key_size = self
            .inner
            .surfaces
            .get(surface_id)
            .await
            .map(|s| {
                let c = &s.descriptor().capabilities;
                (c.key_width_px, c.key_height_px)
            })
            .unwrap_or((72, 72));
        let (width, height) = key_size;
        let flash = solid_key_png(width, height, rgb.0, rgb.1, rgb.2);
        let flash_visual = VisualState {
            image: Some(flash),
            ..Default::default()
        };
        self.push_cell_render(surface_id, row, col, flash_visual)
            .await;
        tokio::time::sleep(Duration::from_millis(450)).await;
        let page_id = self
            .inner
            .page_for_surface(surface_id)
            .or(self.inner.profile.active_page_id)
            .unwrap_or_else(PageId::new);
        let restored = self
            .inner
            .cell_visuals
            .get(&(page_id, row, col))
            .cloned()
            .unwrap_or_default();
        self.push_cell_render(surface_id, row, col, restored).await;
    }

    fn surface_id_for_cell(&self, row: u32, column: u32) -> SurfaceId {
        if let Some(page_id) = self.inner.profile.active_page_id {
            if let Some(page) = self.inner.profile.pages.iter().find(|p| p.id == page_id) {
                for slot in page.slots.values() {
                    if slot.locator.row == row && slot.locator.column == column {
                        return slot.locator.surface_id;
                    }
                }
            }
        }
        self.inner
            .profile
            .surfaces
            .first()
            .map(|s| s.surface_id)
            .unwrap_or_else(SurfaceId::new)
    }

    pub async fn get_cell_visuals(&self) -> std::collections::HashMap<String, VisualState> {
        let Some(page_id) = self.inner.profile.active_page_id else {
            return std::collections::HashMap::new();
        };
        let Some(page) = self.inner.profile.pages.iter().find(|p| p.id == page_id) else {
            return std::collections::HashMap::new();
        };
        let mut out = std::collections::HashMap::new();
        for slot in page.slots.values() {
            let visual = self.effective_visual(page_id, slot);
            out.insert(
                format!("{},{}", slot.locator.row, slot.locator.column),
                visual,
            );
        }
        out
    }

    pub fn property_inspector_path(
        &self,
        plugin_uuid: &str,
        action_uuid: &str,
    ) -> Option<std::path::PathBuf> {
        let plugin_dir = self.loaded_plugin_dir(plugin_uuid)?;
        let manifest = StreamDeckManifest::load(plugin_dir).ok()?;
        let action = manifest.action_by_uuid(action_uuid)?;
        let rel = action.property_inspector.as_ref()?;
        Some(plugin_dir.join(rel))
    }

    pub fn pi_websocket_port(&self, plugin_uuid: &str) -> Option<u16> {
        self.inner
            .sd_supervisor
            .meta(plugin_uuid)
            .map(|p| p.port)
    }

    pub fn any_pi_websocket_port(&self) -> Option<u16> {
        self.inner.sd_supervisor.list().first().map(|p| p.port)
    }

    pub fn get_pi_context(&self, slot_id: SlotId) -> Option<PiContextDto> {
        let context = self.inner.routing.slot_to_context.get(&slot_id)?.clone();
        let page_id = self.page_id_for_slot(slot_id)?;
        let page = self.inner.profile.pages.iter().find(|p| p.id == page_id)?;
        let slot = page.slots.get(&slot_id)?;
        let (plugin_uuid, action_uuid) = match &slot.binding {
            Some(Binding {
                kind:
                    BindingKind::StreamDeck {
                        plugin_uuid,
                        action_uuid,
                        ..
                    },
            }) => (plugin_uuid.clone(), action_uuid.clone()),
            _ => return None,
        };
        let meta = self.inner.sd_supervisor.meta(&plugin_uuid)?;
        let settings = slot
            .binding
            .as_ref()
            .and_then(|b| match &b.kind {
                BindingKind::StreamDeck { settings, .. } => Some(settings.clone()),
                _ => None,
            })
            .unwrap_or_else(|| json!({}));
        Some(PiContextDto {
            port: meta.port,
            context,
            action_uuid,
            plugin_uuid: plugin_uuid.clone(),
            device_id: format!("integratedeck-virtual-{}", plugin_uuid.replace('.', "-")),
            settings,
        })
    }

    pub async fn focus_pi_slot(&mut self, slot_id: Option<SlotId>) -> anyhow::Result<()> {
        if let Some(old_ctx) = self.last_pi_context.take() {
            if let Some(old_uuid) = self.last_pi_plugin_uuid.take() {
                if let Some(broker) = self.inner.sd_supervisor.get(&old_uuid) {
                    broker.property_inspector_did_disappear(&old_ctx).await.ok();
                }
            }
        }
        if let Some(slot_id) = slot_id {
            if let Some(ctx) = self.inner.routing.slot_to_context.get(&slot_id).cloned() {
                let page_id = self
                    .page_id_for_slot(slot_id)
                    .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
                let page = self
                    .inner
                    .profile
                    .pages
                    .iter()
                    .find(|p| p.id == page_id)
                    .ok_or_else(|| anyhow::anyhow!("page not found"))?;
                let slot = page
                    .slots
                    .get(&slot_id)
                    .ok_or_else(|| anyhow::anyhow!("slot not found"))?;
                if let Some(Binding {
                    kind:
                        BindingKind::StreamDeck {
                            plugin_uuid,
                            action_uuid,
                            settings,
                            ..
                        },
                }) = &slot.binding
                {
                    if let Some(broker) = self.inner.sd_supervisor.get(plugin_uuid) {
                        broker
                            .register_context(ActionContext {
                                context: ctx.clone(),
                                action_uuid: action_uuid.clone(),
                                settings: settings.clone(),
                                coordinates: (slot.locator.column, slot.locator.row),
                            })
                            .await;
                        broker.property_inspector_did_appear(&ctx).await?;
                    }
                    self.last_pi_plugin_uuid = Some(plugin_uuid.clone());
                }
                self.last_pi_context = Some(ctx);
            }
        }
        Ok(())
    }

    pub fn connections(&self) -> Vec<ConnectionRecord> {
        self.inner.connections.list()
    }

    pub async fn add_connection(&mut self, record: ConnectionRecord) -> anyhow::Result<()> {
        let id = record.id.clone();
        let module_id = record.module_id.clone();
        let config = record.config.clone();
        let label = record.label.clone();
        self.inner.connections.add(record);
        let _ = self.save_connections();
        if let Some(runtime) = &self.inner.comp_runtime {
            let resp = runtime
                .request(
                    "connection.add",
                    json!({
                        "id": id,
                        "moduleId": module_id,
                        "label": label,
                        "config": config
                    }),
                )
                .await?;
            if !resp.ok {
                anyhow::bail!(resp.error.unwrap_or_else(|| "connection.add failed".into()));
            }
        }
        Ok(())
    }

    pub async fn remove_connection(&mut self, connection_id: String) -> anyhow::Result<()> {
        self.inner.connections.remove(&connection_id);
        let _ = self.save_connections();
        if let Some(runtime) = &self.inner.comp_runtime {
            let resp = runtime
                .request(
                    "connection.remove",
                    json!({ "connectionId": connection_id }),
                )
                .await?;
            if !resp.ok {
                anyhow::bail!(resp
                    .error
                    .unwrap_or_else(|| "connection.remove failed".into()));
            }
        }
        Ok(())
    }

    pub async fn execute_companion_action(
        &self,
        connection_id: &str,
        action_id: &str,
        options: serde_json::Value,
    ) -> anyhow::Result<()> {
        let runtime = self
            .inner
            .comp_runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("companion module engine not running"))?;
        let resp = runtime
            .request(
                "connection.executeAction",
                json!({
                    "connectionId": connection_id,
                    "actionId": action_id,
                    "options": options
                }),
            )
            .await?;
        if !resp.ok {
            anyhow::bail!(resp
                .error
                .unwrap_or_else(|| "executeAction failed".into()));
        }
        Ok(())
    }

    pub async fn companion_ping(&self) -> Option<String> {
        let runtime = self.inner.comp_runtime.as_ref()?;
        match runtime.ping().await {
            Ok(v) => Some(v.to_string()),
            Err(e) => {
                warn!("companion ping failed: {e}");
                None
            }
        }
    }

    pub async fn list_action_library(&self) -> anyhow::Result<ActionLibrary> {
        let scan = self.scan_plugins()?;
        let loaded: std::collections::HashMap<String, LoadedSdPlugin> = self
            .inner
            .sd_supervisor
            .list()
            .into_iter()
            .map(|p| (p.plugin_uuid.clone(), p))
            .collect();

        let mut streamdeck = Vec::new();
        for entry in scan.streamdeck {
            let path = entry.path.clone();
            let plugin_path = std::path::Path::new(&path);
            let uuid = entry
                .uuid
                .clone()
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| effective_plugin_uuid(plugin_path, None));
            let running = loaded.contains_key(&uuid);
            let actions = if running {
                self.list_plugin_actions(None, Some(uuid.clone())).await?
            } else {
                StreamDeckManifest::list_actions(plugin_path)
                    .map(sd_manifest_actions)
                    .unwrap_or_default()
            };
            streamdeck.push(PluginLibraryEntry {
                id: uuid.clone(),
                name: entry.name,
                path: entry.path,
                source: "streamdeck".into(),
                status: if running { "running" } else { "stopped" }.into(),
                port: loaded.get(&uuid).map(|p| p.port),
                actions,
            });
        }

        let mut companion = Vec::new();
        for record in self.inner.connections.list() {
            let actions = if record.enabled {
                self.list_plugin_actions(Some(record.id.clone()), None)
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            companion.push(PluginLibraryEntry {
                id: record.id.clone(),
                name: record.label.clone(),
                path: record.module_id.clone(),
                source: "companion".into(),
                status: if record.enabled { "running" } else { "stopped" }.into(),
                port: None,
                actions,
            });
        }

        Ok(ActionLibrary {
            streamdeck: {
                let mut list = streamdeck;
                list.insert(
                    0,
                    PluginLibraryEntry {
                        id: BUILTIN_PLUGIN_ID.into(),
                        name: "Navigation & Multi".into(),
                        path: String::new(),
                        source: "builtin".into(),
                        status: "available".into(),
                        port: None,
                        actions: vec![
                            OPEN_FOLDER,
                            BACK_TO_PARENT,
                            SWITCH_PAGE,
                            MULTI_ACTION,
                        ]
                        .into_iter()
                        .filter_map(|id| {
                            Some(PluginActionInfo {
                                id: id.into(),
                                name: action_display_name(id)?.into(),
                                source: "builtin".into(),
                            })
                        })
                        .collect(),
                    },
                );
                list
            },
            companion,
        })
    }

    pub fn variables(&self) -> std::collections::HashMap<String, String> {
        self.inner.variables.all().clone()
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionLibrary {
    pub streamdeck: Vec<PluginLibraryEntry>,
    pub companion: Vec<PluginLibraryEntry>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginLibraryEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub source: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub actions: Vec<PluginActionInfo>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiContextDto {
    pub port: u16,
    pub context: String,
    pub action_uuid: String,
    pub plugin_uuid: String,
    pub device_id: String,
    pub settings: serde_json::Value,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub streamdeck: Vec<PluginScanEntry>,
    pub companion: Vec<PluginScanEntry>,
    pub streamdeck_dirs: Vec<String>,
    pub companion_dir: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginScanEntry {
    pub path: String,
    pub name: String,
    pub bundle_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl PluginScanEntry {
    fn from_sd_plugin(path: PathBuf) -> Self {
        let bundle_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let summary = StreamDeckManifest::read_summary(&path);
        let uuid = summary
            .as_ref()
            .and_then(|m| m.uuid.clone())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| effective_plugin_uuid(&path, None));
        Self {
            path: path.display().to_string(),
            name: summary
                .as_ref()
                .map(|m| m.name.clone())
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| bundle_name.clone()),
            bundle_name,
            uuid: Some(uuid),
            version: summary.as_ref().and_then(|m| m.version.clone()),
        }
    }

    fn from_companion_dir(path: PathBuf) -> Self {
        let bundle_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Self {
            path: path.display().to_string(),
            name: bundle_name.clone(),
            bundle_name,
            uuid: None,
            version: None,
        }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDeviceEntry {
    pub device_key: String,
    pub serial: String,
    pub kind: String,
    pub product: String,
    pub label: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
    pub brightness: u8,
    pub rows: u32,
    pub columns: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevicePresetExport {
    pub device_key: String,
    pub label: String,
    pub surface_id: String,
    pub profile_name: String,
    pub pages: Vec<DevicePresetPage>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevicePresetPage {
    pub id: String,
    pub name: String,
    pub slots: std::collections::HashMap<String, ideck_core::Slot>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectHidResult {
    pub surface_id: String,
    pub label: String,
    pub rows: u32,
    pub columns: u32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadPluginResult {
    pub plugin_uuid: String,
    pub port: u16,
    pub name: String,
}

fn sd_manifest_actions(
    actions: Vec<ideck_sd_host::ManifestAction>,
) -> Vec<PluginActionInfo> {
    actions
        .into_iter()
        .filter_map(|a| {
            let id = a.uuid?;
            let name = a.name.unwrap_or_else(|| id.clone());
            Some(PluginActionInfo {
                id,
                name,
                source: "streamdeck".into(),
            })
        })
        .collect()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginActionInfo {
    pub id: String,
    pub name: String,
    pub source: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub sd_plugins: Vec<SdPluginRuntime>,
    pub plugins: Vec<PluginRuntimeEntry>,
    pub surfaces: Vec<SurfaceRuntimeEntry>,
    pub usb_devices: Vec<UsbDeviceRuntimeEntry>,
    pub streamdeck_dirs: Vec<String>,
    pub companion: CompanionRuntimeStatus,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdPluginRuntime {
    pub status: String,
    pub path: String,
    pub name: String,
    pub plugin_uuid: String,
    pub port: u16,
    pub pi_url: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRuntimeEntry {
    pub status: String,
    pub path: String,
    pub name: String,
    pub bundle_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pi_url: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceRuntimeEntry {
    pub surface_id: String,
    pub label: String,
    pub backend: String,
    pub status: String,
    pub rows: u32,
    pub columns: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbDeviceRuntimeEntry {
    pub serial: String,
    pub kind: String,
    pub product: String,
    pub rows: u32,
    pub columns: u32,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_id: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionRuntimeStatus {
    pub companion_host_running: bool,
    pub connections: Vec<ConnectionRecord>,
    pub modules: Vec<PluginScanEntry>,
}

fn parse_binding_kind(value: serde_json::Value) -> anyhow::Result<BindingKind> {
    if let Ok(kind) = serde_json::from_value::<BindingKind>(value.clone()) {
        return Ok(kind);
    }
    if let Some(inner) = value.get("kind") {
        if let Ok(kind) = serde_json::from_value::<BindingKind>(inner.clone()) {
            return Ok(kind);
        }
    }
    anyhow::bail!("invalid binding")
}

fn fresh_binding_kind(kind: &BindingKind) -> BindingKind {
    match kind {
        BindingKind::StreamDeck {
            plugin_uuid,
            action_uuid,
            settings,
            ..
        } => BindingKind::StreamDeck {
            plugin_uuid: plugin_uuid.clone(),
            action_uuid: action_uuid.clone(),
            instance_id: ActionInstanceId::new(),
            settings: settings.clone(),
        },
        BindingKind::Companion { .. } | BindingKind::BuiltIn { .. } => kind.clone(),
        BindingKind::MultiAction { steps, delay_ms } => BindingKind::MultiAction {
            steps: steps
                .iter()
                .map(|s| MultiActionStep {
                    binding: fresh_binding_kind(&s.binding),
                    delay_before_ms: s.delay_before_ms,
                })
                .collect(),
            delay_ms: *delay_ms,
        },
    }
}
