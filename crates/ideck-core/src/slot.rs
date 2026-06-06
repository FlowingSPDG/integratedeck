use serde::{Deserialize, Serialize};

use crate::{Binding, SlotLocator};

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
    pub label: Option<String>,
    pub binding: Option<Binding>,
}

impl Slot {
    pub fn new(locator: SlotLocator) -> Self {
        Self {
            id: SlotId::new(),
            locator,
            label: None,
            binding: None,
        }
    }
}
