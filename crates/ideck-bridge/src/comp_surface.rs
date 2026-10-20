use std::collections::HashMap;

use ideck_core::{SurfaceId, VisualState};
use ideck_surface::{CellAddress, CellUpdate, SurfaceInput};

/// Maps Companion feedback and actions to surface cells.
pub struct CompSurfaceBridge {
    /// Two surfaces can share the same (row, column), so the cell index keeps
    /// the surface id.
    pub control_by_cell: HashMap<(SurfaceId, u32, u32), String>,
    pub cell_by_control: HashMap<String, (u32, u32)>,
    pub surface_by_control: HashMap<String, SurfaceId>,
}

impl CompSurfaceBridge {
    pub fn new() -> Self {
        Self {
            control_by_cell: HashMap::new(),
            cell_by_control: HashMap::new(),
            surface_by_control: HashMap::new(),
        }
    }

    /// Register a Companion control together with the surface that owns it.
    pub fn register_cell_for_surface(
        &mut self,
        surface_id: SurfaceId,
        row: u32,
        column: u32,
        control_id: String,
    ) {
        self.surface_by_control
            .insert(control_id.clone(), surface_id);
        self.control_by_cell
            .insert((surface_id, row, column), control_id.clone());
        self.cell_by_control.insert(control_id, (row, column));
    }

    pub fn surface_for_control(&self, control_id: &str) -> Option<SurfaceId> {
        self.surface_by_control.get(control_id).copied()
    }

    /// Forget whatever control was registered for a cell (slot unbound).
    pub fn unregister_cell(&mut self, surface_id: SurfaceId, row: u32, column: u32) {
        if let Some(control_id) = self.control_by_cell.remove(&(surface_id, row, column)) {
            self.cell_by_control.remove(&control_id);
            self.surface_by_control.remove(&control_id);
        }
    }

    pub fn control_for_input(&self, surface_id: SurfaceId, address: &CellAddress) -> Option<&str> {
        self.control_by_cell
            .get(&(surface_id, address.row, address.column))
            .map(String::as_str)
    }

    pub fn feedback_to_update(&self, control_id: &str, visual: VisualState) -> Option<CellUpdate> {
        let (row, col) = self.cell_by_control.get(control_id)?;
        Some(CellUpdate {
            address: CellAddress {
                row: *row,
                column: *col,
            },
            visual,
        })
    }

    pub fn parse_feedback_style(value: &serde_json::Value) -> VisualState {
        let mut visual = VisualState::default();
        if let Some(color) = value.get("bgcolor").and_then(parse_color_value) {
            visual.bgcolor = Some(color);
        }
        if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
            visual.title = Some(text.to_string());
        }
        visual
    }

    pub fn surface_input_control(
        surface_id: SurfaceId,
        input: &SurfaceInput,
        bridge: &Self,
    ) -> Option<String> {
        match input {
            SurfaceInput::KeyDown { address } | SurfaceInput::KeyUp { address } => bridge
                .control_for_input(surface_id, address)
                .map(String::from),
            _ => None,
        }
    }
}

fn parse_color_value(value: &serde_json::Value) -> Option<ideck_core::Color> {
    if let Some(text) = value.as_str() {
        return parse_rgb(text);
    }
    value.as_u64().map(|value| ideck_core::Color {
        r: ((value >> 16) & 0xff) as u8,
        g: ((value >> 8) & 0xff) as u8,
        b: (value & 0xff) as u8,
    })
}

fn parse_rgb(s: &str) -> Option<ideck_core::Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        return Some(ideck_core::Color { r, g, b });
    }
    None
}

impl Default for CompSurfaceBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unregistering_a_cell_drops_every_index() {
        let mut bridge = CompSurfaceBridge::new();
        let surface = SurfaceId(Default::default());
        bridge.register_cell_for_surface(surface, 2, 3, "conn:ctl".into());
        assert_eq!(bridge.surface_for_control("conn:ctl"), Some(surface));

        bridge.unregister_cell(surface, 2, 3);

        assert!(bridge.surface_for_control("conn:ctl").is_none());
        assert!(bridge
            .feedback_to_update("conn:ctl", VisualState::default())
            .is_none());
        assert!(bridge
            .control_for_input(surface, &CellAddress { row: 2, column: 3 })
            .is_none());
    }

    #[test]
    fn two_surfaces_can_share_a_cell_without_clobbering_each_other() {
        let mut bridge = CompSurfaceBridge::new();
        let studio = SurfaceId(uuid::Uuid::new_v4());
        let plus = SurfaceId(uuid::Uuid::new_v4());
        bridge.register_cell_for_surface(studio, 0, 0, "conn:studio".into());
        bridge.register_cell_for_surface(plus, 0, 0, "conn:plus".into());

        let address = CellAddress { row: 0, column: 0 };
        assert_eq!(
            bridge.control_for_input(studio, &address),
            Some("conn:studio")
        );
        assert_eq!(bridge.control_for_input(plus, &address), Some("conn:plus"));

        // Unbinding the Studio key must leave the Plus key untouched.
        bridge.unregister_cell(studio, 0, 0);
        assert!(bridge
            .feedback_to_update("conn:studio", VisualState::default())
            .is_none());
        assert!(bridge
            .feedback_to_update("conn:plus", VisualState::default())
            .is_some());
    }

    #[test]
    fn parses_numeric_and_hex_feedback_colors() {
        let numeric = CompSurfaceBridge::parse_feedback_style(&serde_json::json!({
            "bgcolor": 0x00ff00,
            "text": "live"
        }));
        assert_eq!(
            numeric.bgcolor,
            Some(ideck_core::Color { r: 0, g: 255, b: 0 })
        );
        assert_eq!(numeric.title.as_deref(), Some("live"));

        let hex = CompSurfaceBridge::parse_feedback_style(&serde_json::json!({
            "bgcolor": "#112233"
        }));
        assert_eq!(
            hex.bgcolor,
            Some(ideck_core::Color {
                r: 0x11,
                g: 0x22,
                b: 0x33
            })
        );
    }
}
