use serde::{Deserialize, Serialize};

use crate::{Binding, SlotAppearance, SlotLocator};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SlotId(pub uuid::Uuid);

impl SlotId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

/// A single control cell on a page (button, encoder, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub id: SlotId,
    pub locator: SlotLocator,
    /// Legacy label; prefer `appearance.title`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub appearance: SlotAppearance,
    pub binding: Option<Binding>,
}

impl Slot {
    pub fn new(locator: SlotLocator) -> Self {
        Self {
            id: SlotId::new(),
            locator,
            label: None,
            appearance: SlotAppearance::default(),
            binding: None,
        }
    }

    pub fn effective_title(&self) -> Option<&str> {
        self.appearance
            .title
            .as_deref()
            .filter(|t| !t.is_empty())
            .or(self.label.as_deref().filter(|t| !t.is_empty()))
    }
}
