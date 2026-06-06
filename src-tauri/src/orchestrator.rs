use std::path::PathBuf;
use std::sync::Arc;

use ideck_bridge::SdSurfaceBridge;
use ideck_comp_host::{scan_module_dirs, scan_sd_plugins, ConnectionRecord, SidecarClient};
use ideck_core::{ActionInstanceId, Binding, BindingKind, Profile, SlotId, SurfaceId};
use ideck_sd_host::{ActionContext, BrokerEvent, StreamDeckBroker, StreamDeckManifest};
use ideck_surface::{MockSurface, SurfaceCapabilities, SurfaceInput};
use tauri::AppHandle;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::paths;
use crate::state::AppStateInner;

pub struct Orchestrator {
    inner: AppStateInner,
    broker_events: Option<mpsc::UnboundedReceiver<BrokerEvent>>,
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
            broker_events: None,
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
        id
    }

    pub async fn list_surface_descriptors(&self) -> Vec<ideck_surface::SurfaceDescriptor> {
        self.inner.surfaces.list().await
    }

    pub fn scan_plugins(&self) -> ScanResult {
        ScanResult {
            streamdeck: scan_sd_plugins(&paths::sd_plugins_dir()),
            companion: scan_module_dirs(&paths::companion_modules_dir()),
        }
    }

    pub async fn load_sd_plugin(&mut self, plugin_path: PathBuf) -> anyhow::Result<LoadPluginResult> {
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
        self.broker_events = Some(events);

        Ok(LoadPluginResult {
            plugin_uuid: manifest.uuid,
            port,
            name: manifest.name,
        })
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
                    // Process pending broker events
                    self.drain_broker_events().await?;
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
            if let Some((context, _event)) = bridge.surface_input_to_sd_event(&input) {
                broker.key_down(&context).await?;
                self.drain_broker_events().await?;
            }
        }
        let _ = surface_id;
        Ok(())
    }

    async fn drain_broker_events(&mut self) -> anyhow::Result<()> {
        let events: Vec<BrokerEvent> = if let Some(rx) = self.broker_events.as_mut() {
            let mut v = Vec::new();
            while let Ok(ev) = rx.try_recv() {
                v.push(ev);
            }
            v
        } else {
            Vec::new()
        };
        for ev in events {
            self.apply_broker_event(ev).await?;
        }
        Ok(())
    }

    async fn apply_broker_event(&mut self, ev: BrokerEvent) -> anyhow::Result<()> {
        let bridge = self
            .inner
            .bridge
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no bridge"))?;
        let updates = bridge.broker_event_to_updates(&ev, &mut self.inner.cell_visuals);
        for update in updates {
            let surface_id = self
                .inner
                .profile
                .surfaces
                .first()
                .map(|s| s.surface_id)
                .unwrap_or_else(SurfaceId::new);
            self.inner
                .surfaces
                .render(surface_id, &[update])
                .await
                .ok();
        }
        Ok(())
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
pub struct ScanResult {
    pub streamdeck: Vec<PathBuf>,
    pub companion: Vec<PathBuf>,
}

#[derive(serde::Serialize)]
pub struct LoadPluginResult {
    pub plugin_uuid: String,
    pub port: u16,
    pub name: String,
}

// Fix Profile deserialize - I made an error in orchestrator.new
