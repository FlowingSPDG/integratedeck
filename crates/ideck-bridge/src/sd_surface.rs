use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD, Engine};
use ideck_core::{ImageFormat, ImagePayload, SlotLocator, VisualState};
use ideck_sd_host::BrokerEvent;
use ideck_surface::{CellAddress, CellUpdate, EmulationProfile, SurfaceCapabilities, SurfaceInput};
use tracing::debug;

/// Maps surface grid coordinates to Stream Deck action contexts and handles broker events → cell renders.
pub struct SdSurfaceBridge {
    pub emulation: SurfaceCapabilities,
    /// (row, col) -> SD context id
    pub context_by_cell: HashMap<(u32, u32), String>,
    /// context -> (row, col)
    pub cell_by_context: HashMap<String, (u32, u32)>,
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

    pub fn register_cell(&mut self, row: u32, column: u32, context: String) {
        self.context_by_cell.insert((row, column), context.clone());
        self.cell_by_context.insert(context, (row, column));
    }

    pub fn context_for_input(&self, address: &CellAddress) -> Option<&str> {
        self.context_by_cell
            .get(&(address.row, address.column))
            .map(String::as_str)
    }

    pub fn surface_input_to_sd_event(
        &self,
        input: &SurfaceInput,
    ) -> Option<(String, &'static str)> {
        match input {
            SurfaceInput::KeyDown { address } => self
                .context_for_input(address)
                .map(|c| (c.to_string(), "keyDown")),
            SurfaceInput::KeyUp { address } => self
                .context_for_input(address)
                .map(|c| (c.to_string(), "keyUp")),
            SurfaceInput::EncoderRotate { index, ticks, pressed } => {
                let ctx = self
                    .context_by_cell
                    .get(&(0, *index))
                    .cloned()
                    .or_else(|| self.context_by_cell.get(&(*index, 0)).cloned())?;
                let _ = (ticks, pressed);
                Some((ctx, "dialRotate"))
            }
            SurfaceInput::EncoderPress { index, pressed: _ } => {
                let ctx = self
                    .context_by_cell
                    .get(&(0, *index))
                    .cloned()
                    .or_else(|| self.context_by_cell.get(&(*index, 0)).cloned())?;
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
                image_base64,
                state,
            } => {
                if let Some((row, col)) = self.cell_by_context.get(context) {
                    let visual = visuals.entry((*row, *col)).or_default();
                    visual.state_index = *state;
                    if let Some(b64) = image_base64 {
                        if let Ok(data) = STANDARD.decode(b64) {
                            visual.image = Some(ImagePayload {
                                format: ImageFormat::Png,
                                data: scale_image_for_cell(
                                    &data,
                                    self.emulation.key_width_px,
                                    self.emulation.key_height_px,
                                ),
                            });
                        }
                    }
                    updates.push(CellUpdate {
                        address: CellAddress {
                            row: *row,
                            column: *col,
                        },
                        visual: visual.clone(),
                    });
                }
            }
            BrokerEvent::SetTitle { context, title } => {
                if let Some((row, col)) = self.cell_by_context.get(context) {
                    let visual = visuals.entry((*row, *col)).or_default();
                    visual.title = title.clone();
                    updates.push(CellUpdate {
                        address: CellAddress {
                            row: *row,
                            column: *col,
                        },
                        visual: visual.clone(),
                    });
                }
            }
            BrokerEvent::SetState { context, state } => {
                if let Some((row, col)) = self.cell_by_context.get(context) {
                    let visual = visuals.entry((*row, *col)).or_default();
                    visual.state_index = *state;
                    updates.push(CellUpdate {
                        address: CellAddress { row: *row, column: *col },
                        visual: visual.clone(),
                    });
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
}

fn scale_image_for_cell(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    use image::ImageReader;
    let Ok(img) = ImageReader::new(std::io::Cursor::new(data)).with_guessed_format() else {
        return data.to_vec();
    };
    let Ok(img) = img.decode() else {
        return data.to_vec();
    };
    let resized = image::imageops::resize(
        &img,
        width,
        height,
        image::imageops::FilterType::Lanczos3,
    );
    let mut buf = std::io::Cursor::new(Vec::new());
    if resized
        .write_to(&mut buf, image::ImageFormat::Png)
        .is_ok()
    {
        buf.into_inner()
    } else {
        data.to_vec()
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
