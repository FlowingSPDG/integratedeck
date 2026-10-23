use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{Binding, PageId, ProfileId, Slot, SlotId, SurfaceId};

/// User-facing profile: pages, surface assignment, slots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub surfaces: Vec<SurfaceAssignment>,
    pub pages: Vec<Page>,
    #[serde(default)]
    pub active_page_id: Option<PageId>,
    /// Per-slot Companion feedback binding, keyed by slot id string.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub companion_feedbacks: HashMap<String, CompanionFeedbackConfig>,
    /// Executables that switch to this profile while they run (lowercase, with
    /// or without `.exe`), like Elgato's "smart profiles".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applications: Vec<String>,
}

/// Companion feedback instance attached to a slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CompanionFeedbackConfig {
    pub connection_id: String,
    pub feedback_id: String,
    #[serde(default)]
    pub action_id: String,
    #[serde(default)]
    pub options: serde_json::Value,
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
    /// Parent page when this page is a folder child.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_page_id: Option<PageId>,
    /// Encoder/dial bindings keyed by the physical encoder index.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub encoders: HashMap<u32, Binding>,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        let page = Page {
            id: PageId::new(),
            name: "Page 1".into(),
            slots: HashMap::new(),
            parent_page_id: None,
            encoders: HashMap::new(),
        };
        let active = page.id;
        Self {
            id: ProfileId::new(),
            name: name.into(),
            surfaces: Vec::new(),
            pages: vec![page],
            active_page_id: Some(active),
            companion_feedbacks: HashMap::new(),
            applications: Vec::new(),
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

    /// Page a touch-strip swipe should open. `delta_x < 0` is a right-to-left
    /// swipe (next page); positive is left-to-right (previous page). Siblings
    /// are the pages that share the current page's parent, so swiping inside a
    /// folder stays inside that folder.
    pub fn swipe_target_page(&self, current: PageId, delta_x: i64) -> Option<PageId> {
        if delta_x == 0 {
            return None;
        }
        let parent = self
            .pages
            .iter()
            .find(|page| page.id == current)
            .and_then(|page| page.parent_page_id);
        let siblings: Vec<PageId> = self
            .pages
            .iter()
            .filter(|page| page.parent_page_id == parent)
            .map(|page| page.id)
            .collect();
        if siblings.len() < 2 {
            return None;
        }
        // Match Elgato: swipe walks every sibling page in order, empty ones
        // included. Skipping blanks made the gesture look broken on a profile
        // with a single finished page, and pages are user-created on purpose.
        let index = siblings.iter().position(|id| *id == current)?;
        let len = siblings.len();
        let next = if delta_x < 0 {
            (index + 1) % len
        } else {
            (index + len - 1) % len
        };
        siblings.get(next).copied().filter(|id| *id != current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(name: &str, parent: Option<PageId>) -> Page {
        Page {
            id: PageId::new(),
            name: name.into(),
            slots: HashMap::new(),
            parent_page_id: parent,
            encoders: HashMap::new(),
        }
    }

    fn profile_with(pages: Vec<Page>) -> Profile {
        Profile {
            id: ProfileId::new(),
            name: "test".into(),
            surfaces: Vec::new(),
            active_page_id: pages.first().map(|page| page.id),
            pages,
            companion_feedbacks: HashMap::new(),
            applications: Vec::new(),
        }
    }

    #[test]
    fn swipe_walks_top_level_pages_in_both_directions() {
        let first = page("One", None);
        let second = page("Two", None);
        let third = page("Three", None);
        let (a, b, c) = (first.id, second.id, third.id);
        let profile = profile_with(vec![first, second, third]);

        assert_eq!(profile.swipe_target_page(a, -1), Some(b));
        assert_eq!(profile.swipe_target_page(b, -1), Some(c));
        // Wraps around so a swipe always changes the page.
        assert_eq!(profile.swipe_target_page(c, -1), Some(a));
        assert_eq!(profile.swipe_target_page(a, 1), Some(c));
        assert_eq!(profile.swipe_target_page(c, 1), Some(b));
    }

    #[test]
    fn swipe_stays_inside_the_folder_of_the_current_page() {
        let top = page("Top", None);
        let top_id = top.id;
        let child_one = page("Child One", Some(top_id));
        let child_two = page("Child Two", Some(top_id));
        let (one, two) = (child_one.id, child_two.id);
        let profile = profile_with(vec![top, child_one, child_two]);

        assert_eq!(profile.swipe_target_page(one, -1), Some(two));
        assert_eq!(profile.swipe_target_page(two, -1), Some(one));
        // The folder root has no siblings, so nothing happens there.
        assert_eq!(profile.swipe_target_page(top_id, -1), None);
    }

    #[test]
    fn swipe_needs_movement_and_a_sibling() {
        let only = page("Only", None);
        let only_id = only.id;
        let profile = profile_with(vec![only]);

        assert_eq!(profile.swipe_target_page(only_id, -1), None);
        assert_eq!(profile.swipe_target_page(only_id, 0), None);
    }

    fn with_slot(mut page: Page) -> Page {
        let locator = crate::SlotLocator {
            surface_id: SurfaceId(uuid::Uuid::nil()),
            page_id: page.id,
            row: 0,
            column: 0,
        };
        let slot = Slot::new(locator);
        page.slots.insert(slot.id, slot);
        page
    }

    #[test]
    fn swipe_visits_empty_pages_like_elgato() {
        let first = with_slot(page("One", None));
        let empty = page("Empty", None);
        let third = with_slot(page("Three", None));
        let (a, empty_id, c) = (first.id, empty.id, third.id);
        let profile = profile_with(vec![first, empty, third]);

        assert_eq!(profile.swipe_target_page(a, -1), Some(empty_id));
        assert_eq!(profile.swipe_target_page(empty_id, -1), Some(c));
        assert_eq!(profile.swipe_target_page(c, -1), Some(a));
        assert_eq!(profile.swipe_target_page(a, 1), Some(c));
        assert_eq!(profile.swipe_target_page(empty_id, 1), Some(a));
    }

    #[test]
    fn swipe_reaches_empty_pages_even_when_the_current_one_is_the_only_one_with_content() {
        let only = with_slot(page("Only", None));
        let empty_one = page("Empty One", None);
        let empty_two = page("Empty Two", None);
        let (only_id, empty_id) = (only.id, empty_one.id);
        let profile = profile_with(vec![only, empty_one, empty_two]);

        assert_eq!(profile.swipe_target_page(only_id, -1), Some(empty_id));
        assert_eq!(
            profile.swipe_target_page(only_id, 1),
            profile.pages.last().map(|page| page.id)
        );
    }
}
