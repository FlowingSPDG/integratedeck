use std::collections::HashMap;

use ideck_core::{image_from_plugin_payload, SlotLocator, SurfaceId, VisualState};
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
    /// (surface, dial index) -> SD context id
    pub context_by_encoder: HashMap<(SurfaceId, u32), String>,
    /// context -> every (surface, dial index) showing it. Two surfaces that show
    /// the same page share one action context, so this must be a list.
    pub encoder_by_context: HashMap<String, Vec<(SurfaceId, u32)>>,
    pub device_id: String,
    pub plugin_uuid: String,
}

impl SdSurfaceBridge {
    pub fn new(plugin_uuid: impl Into<String>, emulation: SurfaceCapabilities) -> Self {
        Self {
            emulation,
            context_by_cell: HashMap::new(),
            cell_by_context: HashMap::new(),
            context_by_encoder: HashMap::new(),
            encoder_by_context: HashMap::new(),
            device_id: "integratedeck-virtual-1".into(),
            plugin_uuid: plugin_uuid.into(),
        }
    }

    pub fn register_cell(&mut self, surface_id: SurfaceId, row: u32, column: u32, context: String) {
        let key = (surface_id, row, column);
        self.context_by_cell.insert(key, context.clone());
        self.cell_by_context
            .insert(context, (surface_id, row, column));
    }

    /// Bind a dial context. Dials are not cells: they must never receive key
    /// renders, otherwise a dial title would overwrite the key at (0, index).
    pub fn register_encoder(&mut self, surface_id: SurfaceId, index: u32, context: String) {
        let key = (surface_id, index);
        if let Some(old) = self.context_by_encoder.get(&key) {
            if old != &context {
                if let Some(list) = self.encoder_by_context.get_mut(old) {
                    list.retain(|(sid, idx)| (*sid, *idx) != key);
                    if list.is_empty() {
                        self.encoder_by_context.remove(old);
                    }
                }
            }
        }
        // Drop any stale cell mapping for this index before claiming it as a dial.
        if let Some(old) = self.context_by_cell.get(&(surface_id, 0, index)) {
            self.cell_by_context.remove(old);
        }
        self.context_by_encoder.insert(key, context.clone());
        let entry = self.encoder_by_context.entry(context).or_default();
        if !entry.contains(&key) {
            entry.push(key);
        }
    }

    pub fn encoders_for_context(&self, context: &str) -> &[(SurfaceId, u32)] {
        self.encoder_by_context
            .get(context)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn context_for_input(&self, surface_id: SurfaceId, address: &CellAddress) -> Option<&str> {
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
            SurfaceInput::EncoderRotate {
                index,
                ticks,
                pressed,
            } => {
                let ctx = self.encoder_context(surface_id, *index)?;
                let _ = (ticks, pressed);
                Some((ctx, "dialRotate"))
            }
            SurfaceInput::EncoderPress { index, pressed: _ } => {
                let ctx = self.encoder_context(surface_id, *index)?;
                Some((ctx, "dialPress"))
            }
            _ => None,
        }
    }

    /// Resolve the context bound to a dial, falling back to cell coordinates for
    /// dials that were bound before dial contexts existed.
    fn encoder_context(&self, surface_id: SurfaceId, index: u32) -> Option<String> {
        self.context_by_encoder
            .get(&(surface_id, index))
            .cloned()
            .or_else(|| self.context_by_cell.get(&(surface_id, 0, index)).cloned())
            .or_else(|| self.context_by_cell.get(&(surface_id, index, 0)).cloned())
    }

    pub fn broker_event_to_updates(
        &self,
        event: &BrokerEvent,
        visuals: &mut HashMap<(u32, u32), VisualState>,
    ) -> Vec<CellUpdate> {
        // Dial feedback is not a key render: it belongs to the dial display.
        if let Some(context) = broker_event_context(event) {
            if self.encoder_by_context.contains_key(context) {
                return Vec::new();
            }
        }
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
            BrokerEvent::SetFeedback { context, payload } => {
                if let Some((_surface_id, row, col)) = self.cell_by_context.get(context) {
                    let visual = visuals.entry((*row, *col)).or_default();
                    if let Some(title) = payload.get("title").and_then(|v| v.as_str()) {
                        visual.title = Some(title.to_string());
                    } else if let Some(value) = payload.get("value").and_then(|v| v.as_str()) {
                        visual.title = Some(value.to_string());
                    }
                    if let Some(icon) = payload.get("icon").and_then(|v| v.as_str()) {
                        if let Some(image) = image_from_plugin_payload(icon, None) {
                            visual.image = Some(image);
                        }
                    }
                    updates.push(cell_update(*row, *col, visual.clone()));
                }
            }
            BrokerEvent::SetTriggerDescription { context, payload } => {
                debug!("trigger description for context {context}: {payload}");
            }
            BrokerEvent::SetFeedbackLayout { context, layout } => {
                debug!("feedback layout for context {context}: {layout}");
            }
            BrokerEvent::SettingsChanged {
                context,
                settings: _,
            } => {
                debug!("settings changed for context {context}");
            }
            _ => {}
        }
        updates
    }

    pub fn map_slot_to_sd_coordinates(locator: &SlotLocator, _profile_columns: u32) -> (u32, u32) {
        (locator.column, locator.row)
    }

    /// Merge a dial's feedback into the dial visual store for every surface that
    /// shows it. Dials render to their own display (touch strip / LCD), not to a
    /// key, so this mirrors the cell merge without touching the key grid.
    pub fn apply_encoder_event(
        &self,
        event: &BrokerEvent,
        visuals: &mut HashMap<(SurfaceId, u32), VisualState>,
    ) -> Vec<(SurfaceId, u32, VisualState)> {
        let Some(context) = broker_event_context(event) else {
            return Vec::new();
        };
        if !self.encoder_by_context.contains_key(context) {
            return Vec::new();
        }
        let mut updates = Vec::new();
        // Start from the primary dial so shared contexts keep identical state.
        for (surface_id, index) in self.encoders_for_context(context).iter().copied() {
            let visual = visuals.entry((surface_id, index)).or_default();
            match event {
                BrokerEvent::SetImage { image, state, .. } => {
                    visual.state_index = *state;
                    match image {
                        ImageSetResult::Set(payload) => visual.image = Some(payload.clone()),
                        ImageSetResult::Cleared => visual.image = None,
                        ImageSetResult::Unchanged => {}
                    }
                }
                BrokerEvent::SetTitle {
                    title,
                    title_params,
                    ..
                } => {
                    visual.title = title.clone();
                    if let Some(params) = title_params {
                        visual.title_params = params.clone();
                    }
                }
                BrokerEvent::SetState { state, .. } => {
                    visual.state_index = *state;
                }
                BrokerEvent::SetFeedback { payload, .. } => {
                    if let Some(title) = payload.get("title").and_then(|v| v.as_str()) {
                        visual.title = Some(title.to_string());
                    } else if let Some(value) = payload.get("value") {
                        // Dial plugins commonly send numeric values.
                        let text = match value {
                            serde_json::Value::String(s) => Some(s.clone()),
                            serde_json::Value::Number(n) => Some(n.to_string()),
                            serde_json::Value::Bool(b) => Some(b.to_string()),
                            _ => None,
                        };
                        if let Some(text) = text {
                            visual.title = Some(text);
                        }
                    }
                    if let Some(icon) = payload.get("icon").and_then(|v| v.as_str()) {
                        if let Some(image) = image_from_plugin_payload(icon, None) {
                            visual.image = Some(image);
                        }
                    }
                }
                _ => return Vec::new(),
            }
            updates.push((surface_id, index, visual.clone()));
        }
        updates
    }

    pub fn build_context_id(plugin_uuid: &str, action_uuid: &str, instance_id: &str) -> String {
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

/// Action context carried by a broker event that can target a cell or a dial.
pub fn broker_event_context(event: &BrokerEvent) -> Option<&str> {
    match event {
        BrokerEvent::SetImage { context, .. }
        | BrokerEvent::SetTitle { context, .. }
        | BrokerEvent::SetState { context, .. }
        | BrokerEvent::SetFeedback { context, .. }
        | BrokerEvent::SetFeedbackLayout { context, .. }
        | BrokerEvent::SetTriggerDescription { context, .. }
        | BrokerEvent::SettingsChanged { context, .. } => Some(context),
        _ => None,
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
                CapabilityNegotiation::unsupported(
                    "surface has no encoders for Stream Deck Plus profile",
                )
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
    fn dial_input_resolves_encoder_context() {
        let mut bridge = SdSurfaceBridge::new("uuid", SurfaceCapabilities::mock_3x5());
        let surface = SurfaceId::new();
        let ctx = SdSurfaceBridge::build_context_id("p", "a", &ActionInstanceId::new().0);
        bridge.register_encoder(surface, 1, ctx.clone());

        let rotate = bridge.surface_input_to_sd_event(
            surface,
            &SurfaceInput::EncoderRotate {
                index: 1,
                ticks: 3,
                pressed: false,
            },
        );
        assert_eq!(rotate, Some((ctx.clone(), "dialRotate")));

        let press = bridge.surface_input_to_sd_event(
            surface,
            &SurfaceInput::EncoderPress {
                index: 1,
                pressed: true,
            },
        );
        assert_eq!(press, Some((ctx, "dialPress")));
    }

    #[test]
    fn registering_a_dial_releases_its_key_cell() {
        let mut bridge = SdSurfaceBridge::new("uuid", SurfaceCapabilities::mock_3x5());
        let surface = SurfaceId::new();
        bridge.register_cell(surface, 0, 0, "key-ctx".into());
        bridge.register_encoder(surface, 0, "dial-ctx".into());

        // The dial now owns index 0, so the stale key mapping must be gone.
        assert!(!bridge.cell_by_context.contains_key("key-ctx"));
        assert_eq!(bridge.encoders_for_context("dial-ctx"), &[(surface, 0)][..]);
    }

    #[test]
    fn dial_feedback_does_not_render_to_key_cells() {
        let mut bridge = SdSurfaceBridge::new("uuid", SurfaceCapabilities::mock_3x5());
        let surface = SurfaceId::new();
        bridge.register_encoder(surface, 0, "dial-ctx".into());

        let title_event = BrokerEvent::SetTitle {
            context: "dial-ctx".into(),
            title: Some("3".into()),
            title_params: None,
        };
        let mut cells = HashMap::new();
        assert!(bridge
            .broker_event_to_updates(&title_event, &mut cells)
            .is_empty());

        // ...but the dial store does receive it.
        let mut dials = HashMap::new();
        let updates = bridge.apply_encoder_event(&title_event, &mut dials);
        assert_eq!(updates.len(), 1);
        let (sid, index, visual) = updates[0].clone();
        assert_eq!((sid, index), (surface, 0));
        assert_eq!(visual.title.as_deref(), Some("3"));
        assert_eq!(
            dials.get(&(surface, 0)).and_then(|v| v.title.as_deref()),
            Some("3")
        );
    }

    #[test]
    fn shared_dial_context_updates_every_surface() {
        let mut bridge = SdSurfaceBridge::new("uuid", SurfaceCapabilities::mock_3x5());
        let studio = SurfaceId::new();
        let plus = SurfaceId::new();
        // Two surfaces showing the same page share one action context.
        bridge.register_encoder(studio, 0, "dial-ctx".into());
        bridge.register_encoder(plus, 0, "dial-ctx".into());

        let event = BrokerEvent::SetTitle {
            context: "dial-ctx".into(),
            title: Some("7".into()),
            title_params: None,
        };
        let mut dials = HashMap::new();
        let mut updates = bridge.apply_encoder_event(&event, &mut dials);
        updates.sort_by_key(|(sid, _, _)| sid.0);
        assert_eq!(updates.len(), 2);
        assert!(updates
            .iter()
            .all(|(_, _, visual)| visual.title.as_deref() == Some("7")));
        assert_eq!(dials.len(), 2);
    }

    #[test]
    fn numeric_dial_feedback_value_becomes_title() {
        let mut bridge = SdSurfaceBridge::new("uuid", SurfaceCapabilities::mock_3x5());
        let surface = SurfaceId::new();
        bridge.register_encoder(surface, 2, "dial-ctx".into());

        let event = BrokerEvent::SetFeedback {
            context: "dial-ctx".into(),
            payload: serde_json::json!({ "value": 42 }),
        };
        let mut dials = HashMap::new();
        let updates = bridge.apply_encoder_event(&event, &mut dials);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].1, 2);
        assert_eq!(updates[0].2.title.as_deref(), Some("42"));
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
