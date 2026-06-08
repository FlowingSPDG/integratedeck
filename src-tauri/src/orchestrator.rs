use std::path::PathBuf;

use ideck_bridge::SdSurfaceBridge;
use ideck_comp_host::{scan_module_dirs, scan_sd_plugins_roots, ConnectionRecord, SidecarClient};
use ideck_core::{ActionInstanceId, Binding, BindingKind, Profile, SlotId, SurfaceId, VisualState};
use ideck_sd_host::{ActionContext, BrokerEvent, StreamDeckBroker, StreamDeckManifest};
use ideck_surface::{
    scan_hid_devices, HidDeviceDescriptor, HidSession, MockSurface, StreamDeckHidSurface,
    SurfaceCapabilities, SurfaceInput, parse_kind_id,
};
use tauri::AppHandle;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::errors::friendly_message;
use crate::paths;
use crate::state::AppStateInner;

pub struct Orchestrator {
    inner: AppStateInner,
    broker_drain_handle: Option<tokio::task::JoinHandle<()>>,
    loaded_plugin: Option<LoadedPluginState>,
    hid_sessions: std::collections::HashMap<SurfaceId, HidSession>,
    hid_serials: std::collections::HashMap<String, SurfaceId>,
    hid_meta: std::collections::HashMap<SurfaceId, HidSurfaceMeta>,
    hid_task_handles: std::collections::HashMap<SurfaceId, Vec<tokio::task::JoinHandle<()>>>,
}

#[derive(Debug, Clone)]
struct LoadedPluginState {
    path: PathBuf,
    name: String,
    plugin_uuid: String,
    port: u16,
}

#[derive(Debug, Clone)]
struct HidSurfaceMeta {
    serial: String,
    kind: String,
    product: String,
}

impl Orchestrator {
    pub async fn new(_app: &AppHandle) -> anyhow::Result<Self> {
        paths::ensure_dirs()?;
        let profile_path = paths::profiles_dir().join("default.json");
        let profile = if profile_path.exists() {
            let data = std::fs::read_to_string(&profile_path)?;
            serde_json::from_str::<Profile>(&data).unwrap_or_else(|_| Profile::new("Default"))
        } else {
            Profile::new("Default")
        };

        let mut inner = AppStateInner::new(profile);
        for assignment in inner.profile.surfaces.clone() {
            let mock = MockSurface::with_id(assignment.surface_id, assignment.label.clone());
            inner.surfaces.register_mock(mock).await;
        }
        let sidecar_dir = paths::sidecar_dir();
        let node = paths::node_binary();
        if sidecar_dir.join("dist/index.js").exists() {
            match SidecarClient::spawn(&sidecar_dir, &node).await {
                Ok((client, _rx)) => {
                    info!("sidecar started");
                    inner.sidecar = Some(client);
                }
                Err(e) => warn!("sidecar not started: {e}"),
            }
        }

        Ok(Self {
            inner,
            broker_drain_handle: None,
            loaded_plugin: None,
            hid_sessions: std::collections::HashMap::new(),
            hid_serials: std::collections::HashMap::new(),
            hid_meta: std::collections::HashMap::new(),
            hid_task_handles: std::collections::HashMap::new(),
        })
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

    pub fn scan_hid_devices(&self) -> Result<Vec<HidDeviceDescriptor>, String> {
        scan_hid_devices().map_err(|e| friendly_message(&e))
    }

    pub async fn connect_hid_device(
        &mut self,
        serial: String,
        kind_id: String,
    ) -> Result<(ConnectHidResult, std::sync::Arc<StreamDeckHidSurface>), String> {
        let kind =
            parse_kind_id(&kind_id).ok_or_else(|| format!("unknown device kind: {kind_id}"))?;
        if self.hid_serials.contains_key(&serial) {
            return Err("device already connected".into());
        }

        let session = HidSession::open(kind, &serial).map_err(|e| friendly_message(&e))?;
        let _ = session.device.set_brightness(50).await;
        let product = session
            .device
            .product()
            .await
            .unwrap_or_else(|_| kind_id.clone());
        let label = format!("{product} ({serial})");
        let surface_id = SurfaceId::new();
        let surface = StreamDeckHidSurface::new(surface_id, &session, label.clone());
        self.inner.surfaces.register_hid(surface.clone()).await;

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
        }
        let _ = self.save_profile().await;

        self.hid_sessions.insert(surface_id, session);
        self.hid_serials.insert(serial.clone(), surface_id);
        self.hid_meta.insert(
            surface_id,
            HidSurfaceMeta {
                serial,
                kind: kind_id,
                product: product.clone(),
            },
        );

        let result = ConnectHidResult {
            surface_id: surface_id.0.to_string(),
            label,
            rows: kind.row_count() as u32,
            columns: kind.column_count() as u32,
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

    pub async fn load_sd_plugin(
        &mut self,
        plugin_path: PathBuf,
    ) -> anyhow::Result<(LoadPluginResult, mpsc::UnboundedReceiver<BrokerEvent>)> {
        if let Some(handle) = self.broker_drain_handle.take() {
            handle.abort();
        }
        self.inner.sd_broker = None;
        self.inner.bridge = None;
        self.loaded_plugin = None;

        let manifest = StreamDeckManifest::load(&plugin_path)?;
        let node = paths::node_binary();
        let (broker, events) = StreamDeckBroker::start(&plugin_path, &node).await?;
        let port = broker.port();
        self.inner.pi_port = Some(port);
        self.inner.sd_broker = Some(broker.clone());
        self.inner.bridge = Some(SdSurfaceBridge::new(
            manifest.uuid.clone(),
            SurfaceCapabilities::streamdeck_mk2(),
        ));
        let result = LoadPluginResult {
            plugin_uuid: manifest.uuid.clone(),
            port,
            name: manifest.name.clone(),
        };
        self.loaded_plugin = Some(LoadedPluginState {
            path: plugin_path,
            name: manifest.name,
            plugin_uuid: manifest.uuid,
            port,
        });
        Ok((result, events))
    }

    pub async fn unload_sd_plugin(&mut self) -> Result<(), String> {
        if let Some(handle) = self.broker_drain_handle.take() {
            handle.abort();
        }
        self.inner.sd_broker = None;
        self.inner.bridge = None;
        self.inner.pi_port = None;
        self.loaded_plugin = None;
        Ok(())
    }

    pub async fn disconnect_hid_device(&mut self, surface_id: SurfaceId) -> Result<(), String> {
        if !self.hid_sessions.contains_key(&surface_id) {
            return Err("指定の物理デバイスは接続されていません。".into());
        }
        if let Some(handles) = self.hid_task_handles.remove(&surface_id) {
            for handle in handles {
                handle.abort();
            }
        }
        self.hid_sessions.remove(&surface_id);
        self.hid_meta.remove(&surface_id);
        self.hid_serials.retain(|_, id| *id != surface_id);
        self.inner.surfaces.unregister(surface_id).await;
        self.inner.profile.surfaces.retain(|s| s.surface_id != surface_id);
        let _ = self.save_profile().await;
        Ok(())
    }

    pub async fn runtime_status(&self) -> anyhow::Result<RuntimeStatus> {
        let scan = self.scan_plugins()?;
        let running_path = self.loaded_plugin.as_ref().map(|p| p.path.display().to_string());
        let plugins: Vec<PluginRuntimeEntry> = scan
            .streamdeck
            .into_iter()
            .map(|entry| {
                let running = running_path.as_deref() == Some(entry.path.as_str());
                PluginRuntimeEntry {
                    status: if running { "running" } else { "stopped" }.into(),
                    path: entry.path,
                    name: entry.name,
                    bundle_name: entry.bundle_name,
                    uuid: entry.uuid,
                    version: entry.version,
                    port: if running {
                        self.loaded_plugin.as_ref().map(|p| p.port)
                    } else {
                        None
                    },
                    pi_url: if running {
                        self.inner.pi_port.map(|p| format!("ws://127.0.0.1:{p}"))
                    } else {
                        None
                    },
                }
            })
            .collect();

        let descriptors = self.list_surface_descriptors().await;
        let mut surfaces = Vec::new();
        for desc in descriptors {
            let backend = match desc.backend {
                ideck_surface::SurfaceBackend::Mock => "mock",
                ideck_surface::SurfaceBackend::StreamDeckHid => "stream_deck_hid",
                ideck_surface::SurfaceBackend::CompanionSidecar { .. } => "companion_sidecar",
            };
            let (serial, kind, product) = self
                .hid_meta
                .get(&desc.id)
                .map(|m| (Some(m.serial.clone()), Some(m.kind.clone()), Some(m.product.clone())))
                .unwrap_or((None, None, None));
            let status = if backend == "stream_deck_hid" && self.hid_sessions.contains_key(&desc.id) {
                "connected"
            } else if backend == "mock" {
                "connected"
            } else {
                "disconnected"
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

        let usb_devices: Vec<UsbDeviceRuntimeEntry> = scan_hid_devices()
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

        let sd_plugin = self.loaded_plugin.as_ref().map(|p| SdPluginRuntime {
            status: "running".into(),
            path: p.path.display().to_string(),
            name: p.name.clone(),
            plugin_uuid: p.plugin_uuid.clone(),
            port: p.port,
            pi_url: format!("ws://127.0.0.1:{}", p.port),
        });

        Ok(RuntimeStatus {
            sd_plugin,
            plugins,
            surfaces,
            usb_devices,
            streamdeck_dirs: scan.streamdeck_dirs,
            companion: CompanionRuntimeStatus {
                sidecar_running: self.inner.sidecar.is_some(),
                connections: self.connections(),
                modules: scan.companion,
            },
        })
    }

    pub fn loaded_plugin_dir(&self) -> Option<&PathBuf> {
        self.loaded_plugin.as_ref().map(|p| &p.path)
    }

    pub fn set_broker_drain_handle(&mut self, handle: tokio::task::JoinHandle<()>) {
        if let Some(existing) = self.broker_drain_handle.replace(handle) {
            existing.abort();
        }
    }

    pub async fn bind_slot_streamdeck(
        &mut self,
        slot_id: SlotId,
        plugin_uuid: String,
        action_uuid: String,
    ) -> anyhow::Result<String> {
        let instance_id = ActionInstanceId::new();
        let context =
            ideck_bridge::SdSurfaceBridge::build_context_id(&plugin_uuid, &action_uuid, &instance_id.0);

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

        if let Some(bridge) = self.inner.bridge.as_mut() {
            bridge.register_cell(row, col, context.clone());
        }

        if let Some(broker) = &self.inner.sd_broker {
            broker
                .register_context(ActionContext {
                    context: context.clone(),
                    action_uuid: action_uuid.clone(),
                    settings: serde_json::json!({}),
                    coordinates: (col, row),
                })
                .await;
            broker.will_appear(&context).await?;
        }

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

        let _ = surface_id;
        Ok(context)
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

        match binding {
            Some(Binding {
                kind: BindingKind::StreamDeck { .. },
            }) => {
                let context = self
                    .inner
                    .bridge
                    .as_ref()
                    .and_then(|b| {
                        let page_id = self.inner.profile.active_page_id?;
                        let page = self.inner.profile.pages.iter().find(|p| p.id == page_id)?;
                        let slot = page.slots.get(&slot_id)?;
                        b.context_by_cell
                            .get(&(slot.locator.row, slot.locator.column))
                            .cloned()
                    })
                    .ok_or_else(|| anyhow::anyhow!("no context for slot"))?;

                if let Some(broker) = &self.inner.sd_broker {
                    broker.key_down(&context).await?;
                    broker.key_up(&context).await?;
                }
            }
            Some(Binding {
                kind:
                    BindingKind::Companion {
                        connection_id,
                        action_id,
                        options,
                    },
            }) => {
                if let Some(sidecar) = &self.inner.sidecar {
                    sidecar.request(
                        "connection.executeAction",
                        serde_json::json!({
                            "connectionId": connection_id,
                            "actionId": action_id,
                            "options": options
                        }),
                    );
                }
            }
            None => {}
        }
        Ok(())
    }

    pub async fn apply_surface_input(
        &mut self,
        surface_id: SurfaceId,
        input: SurfaceInput,
    ) -> anyhow::Result<()> {
        if let (Some(bridge), Some(broker)) = (&self.inner.bridge, &self.inner.sd_broker) {
            match bridge.surface_input_to_sd_event(&input) {
                Some((context, "keyDown")) => {
                    broker.key_down(&context).await?;
                }
                Some((context, "keyUp")) => {
                    broker.key_up(&context).await?;
                }
                _ => {}
            }
        }
        let _ = surface_id;
        Ok(())
    }

    pub async fn apply_broker_event(&mut self, ev: BrokerEvent) -> anyhow::Result<()> {
        let bridge = self
            .inner
            .bridge
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no bridge"))?;
        let updates = bridge.broker_event_to_updates(&ev, &mut self.inner.cell_visuals);
        for update in updates {
            let surface_id = self.surface_id_for_cell(update.address.row, update.address.column);
            self.inner
                .surfaces
                .render(surface_id, &[update])
                .await
                .ok();
        }
        Ok(())
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
        let mut out = std::collections::HashMap::new();
        for ((row, col), visual) in &self.inner.cell_visuals {
            out.insert(format!("{row},{col}"), visual.clone());
        }
        out
    }

    pub fn property_inspector_path(
        &self,
        action_uuid: &str,
    ) -> Option<std::path::PathBuf> {
        let plugin_dir = self.loaded_plugin.as_ref().map(|p| &p.path)?;
        let manifest = StreamDeckManifest::load(plugin_dir).ok()?;
        let action = manifest.action_by_uuid(action_uuid)?;
        let rel = action.property_inspector.as_ref()?;
        Some(plugin_dir.join(rel))
    }

    pub fn pi_websocket_port(&self) -> Option<u16> {
        self.inner.pi_port
    }

    pub fn connections(&self) -> Vec<ConnectionRecord> {
        self.inner.connections.list()
    }

    pub fn add_connection(&mut self, record: ConnectionRecord) {
        let id = record.id.clone();
        let module_id = record.module_id.clone();
        let config = record.config.clone();
        self.inner.connections.add(record);
        if let Some(sidecar) = &self.inner.sidecar {
            sidecar.request(
                "connection.add",
                serde_json::json!({
                    "id": id,
                    "moduleId": module_id,
                    "config": config
                }),
            );
        }
    }

    pub fn sidecar_ping(&self) -> Option<String> {
        self.inner.sidecar.as_ref().map(|s| s.ping())
    }

    pub fn execute_companion_via_sidecar(
        &self,
        connection_id: &str,
        action_id: &str,
        options: serde_json::Value,
    ) {
        if let Some(sidecar) = &self.inner.sidecar {
            sidecar.request(
                "connection.executeAction",
                serde_json::json!({
                    "connectionId": connection_id,
                    "actionId": action_id,
                    "options": options
                }),
            );
        }
    }
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
    /// Human-readable name from manifest.json when available.
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
        let summary = StreamDeckManifest::read_summary(&path)
            .or_else(|| StreamDeckManifest::load(&path).ok().map(|m| {
                ideck_sd_host::ManifestSummary {
                    name: m.name,
                    uuid: Some(m.uuid),
                    version: Some(m.version),
                }
            }));
        Self {
            path: path.display().to_string(),
            name: summary
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| bundle_name.clone()),
            bundle_name,
            uuid: summary.as_ref().and_then(|m| m.uuid.clone()),
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub sd_plugin: Option<SdPluginRuntime>,
    pub plugins: Vec<PluginRuntimeEntry>,
    pub surfaces: Vec<SurfaceRuntimeEntry>,
    pub usb_devices: Vec<UsbDeviceRuntimeEntry>,
    pub streamdeck_dirs: Vec<String>,
    pub companion: CompanionRuntimeStatus,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdPluginRuntime {
    /// running | stopped
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
    /// available | connected
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_id: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionRuntimeStatus {
    pub sidecar_running: bool,
    pub connections: Vec<ConnectionRecord>,
    pub modules: Vec<PluginScanEntry>,
}
