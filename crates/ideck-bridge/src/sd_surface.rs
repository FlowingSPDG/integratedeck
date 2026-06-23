use std::collections::HashMap;

use ideck_core::{SlotLocator, SurfaceId, VisualState};
use ideck_sd_host::{BrokerEvent, ImageSetResult};
use ideck_surface::{CellAddress, CellUpdate, EmulationProfile, SurfaceCapabilities, SurfaceInput};
use tracing::debug;

/// Maps surface grid coordinates to Stream Deck action contexts and handles broker events → cell renders.
pub struct SdSurfaceBridge {
    pub emulation: SurfaceCapabilities,
    /// (surface, row, col) -> SD context id
    pub context_by_cell: HashMap<(SurfaceId, u32, u32), String>,
    /// context -> (surface, row, col)
    pub cell_by_context: HashMap<String, (SurfaceId, u32, u32)>,
    pub device_id: String,
    pub plugin_uuid: String,
}

impl SdSurfaceBridge {
    pub fn new(plugin_uuid: impl Into<String>, emulation: SurfaceCapabilities) -> Self {
        Self {
            emulation,
            context_by_cell: HashMap::new(),
            cell_by_context: HashMap::new(),
            device_id: "integratedeck-virtual-1".into(),
            plugin_uuid: plugin_uuid.into(),
        }
    }

    pub fn register_cell(
        &mut self,
        surface_id: SurfaceId,
        row: u32,
        column: u32,
        context: String,
    ) {
        let key = (surface_id, row, column);
        self.context_by_cell.insert(key, context.clone());
        self.cell_by_context.insert(context, (surface_id, row, column));
    }

    pub fn context_for_input(
        &self,
        surface_id: SurfaceId,
        address: &CellAddress,
    ) -> Option<&str> {
        self.context_by_cell
            .get(&(surface_id, address.row, address.column))
            .map(String::as_str)
    }

    pub fn surface_input_to_sd_event(
        &self,
        surface_id: SurfaceId,
        input: &SurfaceInput,
    ) -> Option<(String, &'static str)> {
        match input {
            SurfaceInput::KeyDown { address } => self
                .context_for_input(surface_id, address)
                .map(|c| (c.to_string(), "keyDown")),
            SurfaceInput::KeyUp { address } => self
                .context_for_input(surface_id, address)
                .map(|c| (c.to_string(), "keyUp")),
            SurfaceInput::EncoderRotate { index, ticks, pressed } => {
                let ctx = self
                    .context_by_cell
                    .get(&(surface_id, 0, *index))
                    .cloned()
                    .or_else(|| {
                        self.context_by_cell
                            .get(&(surface_id, *index, 0))
                            .cloned()
                    })?;
                let _ = (ticks, pressed);
                Some((ctx, "dialRotate"))
            }
            SurfaceInput::EncoderPress { index, pressed: _ } => {
                let ctx = self
                    .context_by_cell
                    .get(&(surface_id, 0, *index))
                    .cloned()
                    .or_else(|| {
                        self.context_by_cell
                            .get(&(surface_id, *index, 0))
                            .cloned()
                    })?;
                Some((ctx, "dialPress"))
            }
            _ => None,
        }
    }

    pub fn broker_event_to_updates(
        &self,
        event: &BrokerEvent,
        visuals: &mut HashMap<(u32, u32), VisualState>,
    ) -> Vec<CellUpdate> {
        let mut updates = Vec::new();
        match event {
            BrokerEvent::SetImage {
                context,
                image,
                state,
            } => {
                if let Some((_surface_id, row, col)) = self.cell_by_context.get(context) {
                    let visual = visuals.entry((*row, *col)).or_default();
                    visual.state_index = *state;
                    match image {
                        ImageSetResult::Set(payload) => visual.image = Some(payload.clone()),
                        ImageSetResult::Cleared => visual.image = None,
                        ImageSetResult::Unchanged => {}
                    }
                    updates.push(cell_update(*row, *col, visual.clone()));
                }
            }
            BrokerEvent::SetTitle {
                context,
                title,
                title_params,
            } => {
                if let Some((_surface_id, row, col)) = self.cell_by_context.get(context) {
                    let visual = visuals.entry((*row, *col)).or_default();
                    visual.title = title.clone();
                    if let Some(params) = title_params {
                        visual.title_params = params.clone();
                    }
                    updates.push(cell_update(*row, *col, visual.clone()));
                }
            }
            BrokerEvent::SetState { context, state } => {
                if let Some((_surface_id, row, col)) = self.cell_by_context.get(context) {
                    let visual = visuals.entry((*row, *col)).or_default();
                    visual.state_index = *state;
                    updates.push(cell_update(*row, *col, visual.clone()));
                }
            }
            BrokerEvent::SettingsChanged { context, settings: _ } => {
                debug!("settings changed for context {context}");
            }
            _ => {}
        }
        updates
    }

    pub fn map_slot_to_sd_coordinates(
        locator: &SlotLocator,
        _profile_columns: u32,
    ) -> (u32, u32) {
        (locator.column, locator.row)
    }

    pub fn build_context_id(
        plugin_uuid: &str,
        action_uuid: &str,
        instance_id: &str,
    ) -> String {
        format!("{plugin_uuid}.{action_uuid}.{instance_id}")
    }

    /// Resolve which loaded plugin UUID owns a full action context id.
    ///
    /// Context ids are `{plugin_uuid}.{action_uuid}.{instance_id}` where both
    /// plugin and action UUIDs may contain dots, so splitting on `.` is invalid.
    pub fn plugin_uuid_for_context(context: &str, loaded_uuids: &[String]) -> Option<String> {
        loaded_uuids
            .iter()
            .filter(|id| {
                !id.is_empty() && (context.starts_with(&format!("{id}.")) || context == **id)
            })
            .max_by_key(|id| id.len())
            .cloned()
    }
}

fn cell_update(row: u32, col: u32, visual: VisualState) -> CellUpdate {
    CellUpdate {
        address: CellAddress { row, column: col },
        visual,
    }
}

/// Capability negotiation result when bridging incompatible features.
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityNegotiation {
    pub supported: bool,
    pub reason: Option<String>,
}

impl CapabilityNegotiation {
    pub fn ok() -> Self {
        Self {
            supported: true,
            reason: None,
        }
    }

    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            supported: false,
            reason: Some(reason.into()),
        }
    }

    pub fn check_emulation(profile: EmulationProfile, caps: &SurfaceCapabilities) -> Self {
        match profile {
            EmulationProfile::Custom => CapabilityNegotiation::ok(),
            EmulationProfile::StreamDeckPlus if caps.encoders == 0 => {
                CapabilityNegotiation::unsupported("surface has no encoders for Stream Deck Plus profile")
            }
            _ => CapabilityNegotiation::ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ideck_core::ActionInstanceId;

    #[test]
    fn key_down_resolves_context_for_surface_and_cell() {
        let mut bridge = SdSurfaceBridge::new("uuid", SurfaceCapabilities::mock_3x5());
        let surface = SurfaceId::new();
        let ctx = SdSurfaceBridge::build_context_id("p", "a", &ActionInstanceId::new().0);
        bridge.register_cell(surface, 1, 2, ctx.clone());

        let ev = bridge.surface_input_to_sd_event(
            surface,
            &SurfaceInput::KeyDown {
                address: CellAddress { row: 1, column: 2 },
            },
        );
        assert_eq!(ev, Some((ctx, "keyDown")));

        let other_surface = SurfaceId::new();
        assert!(bridge
            .surface_input_to_sd_event(
                other_surface,
                &SurfaceInput::KeyDown {
                    address: CellAddress { row: 1, column: 2 },
                },
            )
            .is_none());
    }

    #[test]
    fn plugin_uuid_for_context_supports_dotted_uuids() {
        let loaded = vec![
            "com.elgato.streamdeck.timer".to_string(),
            "dev.flowing.testplugin".to_string(),
        ];
        let ctx = SdSurfaceBridge::build_context_id(
            "com.elgato.streamdeck.timer",
            "com.elgato.streamdeck.timer.action",
            &ActionInstanceId::new().0,
        );
        assert_eq!(
            SdSurfaceBridge::plugin_uuid_for_context(&ctx, &loaded).as_deref(),
            Some("com.elgato.streamdeck.timer")
        );

        let dev_ctx = SdSurfaceBridge::build_context_id(
            "dev.flowing.testplugin",
            "dev.flowing.testplugin.action",
            &ActionInstanceId::new().0,
        );
        assert_eq!(
            SdSurfaceBridge::plugin_uuid_for_context(&dev_ctx, &loaded).as_deref(),
            Some("dev.flowing.testplugin")
        );
    }
}
