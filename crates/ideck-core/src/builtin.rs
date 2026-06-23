//! Built-in Stream Deck–compatible action identifiers (no plugin host required).

pub const PLUGIN_ID: &str = "integratedeck.builtin";

pub const OPEN_FOLDER: &str = "com.elgato.streamdeck.profile.openchild";
pub const BACK_TO_PARENT: &str = "com.elgato.streamdeck.profile.backtoparent";
pub const SWITCH_PAGE: &str = "com.elgato.streamdeck.profile.rotate";
pub const MULTI_ACTION: &str = "com.elgato.streamdeck.multiactions.routine";

pub fn is_builtin_action(action_id: &str) -> bool {
    matches!(
        action_id,
        OPEN_FOLDER | BACK_TO_PARENT | SWITCH_PAGE | MULTI_ACTION
    )
}

pub fn action_display_name(action_id: &str) -> Option<&'static str> {
    match action_id {
        OPEN_FOLDER => Some("Create Folder"),
        BACK_TO_PARENT => Some("Back to Parent"),
        SWITCH_PAGE => Some("Switch Page"),
        MULTI_ACTION => Some("Multi Action"),
        _ => None,
    }
}
