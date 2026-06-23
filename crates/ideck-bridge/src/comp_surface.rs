use std::collections::HashMap;

use ideck_core::VisualState;
use ideck_surface::{CellAddress, CellUpdate, SurfaceInput};

/// Maps Companion feedback and actions to surface cells.
pub struct CompSurfaceBridge {
    pub control_by_cell: HashMap<(u32, u32), String>,
    pub cell_by_control: HashMap<String, (u32, u32)>,
}

impl CompSurfaceBridge {
    pub fn new() -> Self {
        Self {
            control_by_cell: HashMap::new(),
            cell_by_control: HashMap::new(),
        }
    }

    pub fn register_cell(&mut self, row: u32, column: u32, control_id: String) {
        self.control_by_cell
            .insert((row, column), control_id.clone());
        self.cell_by_control.insert(control_id, (row, column));
    }

    pub fn control_for_input(&self, address: &CellAddress) -> Option<&str> {
        self.control_by_cell
            .get(&(address.row, address.column))
            .map(String::as_str)
    }

    pub fn feedback_to_update(
        &self,
        control_id: &str,
        visual: VisualState,
    ) -> Option<CellUpdate> {
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
        if let Some(bg) = value.get("bgcolor").and_then(|v| v.as_str()) {
            if let Some(c) = parse_rgb(bg) {
                visual.bgcolor = Some(c);
            }
        }
        if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
            visual.title = Some(text.to_string());
        }
        visual
    }

    pub fn surface_input_control(input: &SurfaceInput, bridge: &Self) -> Option<String> {
        match input {
            SurfaceInput::KeyDown { address } | SurfaceInput::KeyUp { address } => bridge
                .control_for_input(address)
                .map(String::from),
            _ => None,
        }
    }
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
