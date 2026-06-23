//! Central hub: device events, routing, and Tauri event emission.

use std::collections::HashMap;

use ideck_core::{SlotId, SurfaceId};
use ideck_surface::SurfaceInput;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

/// All surface input events flow through this bus before routing.
pub struct DeviceEventBus {
    tx: broadcast::Sender<DeviceEvent>,
}

#[derive(Debug, Clone)]
pub struct DeviceEvent {
    pub surface_id: SurfaceId,
    pub input: SurfaceInput,
}

impl DeviceEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    pub fn publish(&self, surface_id: SurfaceId, input: SurfaceInput) {
        let _ = self.tx.send(DeviceEvent { surface_id, input });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DeviceEvent> {
        self.tx.subscribe()
    }
}

impl Default for DeviceEventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps grid cells and slots to plugin contexts / companion control ids.
#[derive(Default)]
pub struct RoutingTable {
    pub slot_to_context: HashMap<SlotId, String>,
    pub cell_to_context: HashMap<(u32, u32), String>,
    pub context_to_cell: HashMap<String, (u32, u32)>,
    pub slot_to_companion: HashMap<SlotId, (String, String)>,
}

impl RoutingTable {
    pub fn register_sd_cell(&mut self, row: u32, col: u32, context: String) {
        self.cell_to_context.insert((row, col), context.clone());
        self.context_to_cell.insert(context, (row, col));
    }

    pub fn register_sd_slot(&mut self, slot_id: SlotId, context: String) {
        self.slot_to_context.insert(slot_id, context);
    }

    pub fn register_companion_slot(
        &mut self,
        slot_id: SlotId,
        connection_id: String,
        action_id: String,
    ) {
        self.slot_to_companion
            .insert(slot_id, (connection_id, action_id));
    }

    pub fn clear_slot(&mut self, slot_id: &SlotId) {
        self.slot_to_context.remove(slot_id);
        self.slot_to_companion.remove(slot_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ideck_core::SlotId;

    #[test]
    fn routing_table_registers_sd_and_companion() {
        let mut table = RoutingTable::default();
        let slot = SlotId::new();
        table.register_sd_slot(slot, "ctx-1".into());
        table.register_companion_slot(slot, "conn".into(), "act".into());
        assert_eq!(table.slot_to_context.get(&slot).unwrap(), "ctx-1");
        assert_eq!(
            table.slot_to_companion.get(&slot).unwrap(),
            &("conn".into(), "act".into())
        );
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualUpdatedPayload {
    pub row: u32,
    pub column: u32,
    pub visual: ideck_core::VisualState,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsChangedPayload {
    pub context: String,
    pub settings: serde_json::Value,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatusPayload {
    pub plugin_uuid: String,
    pub status: String,
    pub message: Option<String>,
}

pub struct HubEvents;

impl HubEvents {
    pub const VISUAL_UPDATED: &'static str = "visual-updated";
    pub const SETTINGS_CHANGED: &'static str = "settings-changed";
    pub const DEFINITIONS_UPDATED: &'static str = "definitions-updated";
    pub const PLUGIN_STATUS: &'static str = "plugin-status";
    pub const PI_MESSAGE: &'static str = "pi-message";

    pub fn emit_visual(app: &AppHandle, payload: VisualUpdatedPayload) {
        let _ = app.emit(Self::VISUAL_UPDATED, payload);
    }

    pub fn emit_settings(app: &AppHandle, payload: SettingsChangedPayload) {
        let _ = app.emit(Self::SETTINGS_CHANGED, payload);
    }

    pub fn emit_plugin_status(app: &AppHandle, payload: PluginStatusPayload) {
        let _ = app.emit(Self::PLUGIN_STATUS, payload);
    }

    pub fn emit_pi_message(app: &AppHandle, context: &str, payload: serde_json::Value) {
        let _ = app.emit(
            Self::PI_MESSAGE,
            serde_json::json!({ "context": context, "payload": payload }),
        );
    }
}
