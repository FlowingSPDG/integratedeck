use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{PageId, ProfileId, Slot, SlotId, SurfaceId};

/// User-facing profile: pages, surface assignment, slots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub surfaces: Vec<SurfaceAssignment>,
    pub pages: Vec<Page>,
    #[serde(default)]
    pub active_page_id: Option<PageId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceAssignment {
    pub surface_id: SurfaceId,
    pub label: String,
    /// Which emulation profile to use when bridging SD plugins (e.g. "streamdeck_mk2").
    #[serde(default)]
    pub emulation_profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    pub name: String,
    pub slots: HashMap<SlotId, Slot>,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        let page = Page {
            id: PageId::new(),
            name: "Page 1".into(),
            slots: HashMap::new(),
        };
        let active = page.id;
        Self {
            id: ProfileId::new(),
            name: name.into(),
            surfaces: Vec::new(),
            pages: vec![page],
            active_page_id: Some(active),
        }
    }

    pub fn active_page_mut(&mut self) -> Option<&mut Page> {
        let id = self.active_page_id?;
        self.pages.iter_mut().find(|p| p.id == id)
    }

    pub fn active_page(&self) -> Option<&Page> {
        let id = self.active_page_id?;
        self.pages.iter().find(|p| p.id == id)
    }
}
