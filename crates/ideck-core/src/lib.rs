//! Core domain types: profiles, pages, slots, bindings, visual state.

mod appearance;
mod binding;
mod builtin;
mod key_render;
mod profile;
mod slot;
mod variables;
mod visual;

pub use appearance::*;
pub use key_render::*;
pub use binding::*;
pub use builtin::*;
pub use profile::*;
pub use slot::{Slot, SlotId};
pub use variables::*;
pub use visual::*;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identifier for a physical or virtual control surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SurfaceId(pub Uuid);

impl SurfaceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SurfaceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Stream Deck action instance identifier (per key/context).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionInstanceId(pub String);

impl ActionInstanceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for ActionInstanceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalized location of a control on a surface + page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotLocator {
    pub surface_id: SurfaceId,
    pub page_id: PageId,
    pub row: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PageId(pub Uuid);

impl PageId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PageId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(pub Uuid);

impl ProfileId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProfileId {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_roundtrip_json() {
        let p = Profile::new("Test");
        let json = serde_json::to_string(&p).unwrap();
        let back: Profile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Test");
        assert_eq!(back.pages.len(), 1);
    }
}
