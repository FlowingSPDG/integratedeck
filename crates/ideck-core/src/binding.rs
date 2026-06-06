use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::ActionInstanceId;

/// What a slot executes when pressed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BindingKind {
    StreamDeck {
        plugin_uuid: String,
        action_uuid: String,
        instance_id: ActionInstanceId,
        #[serde(default)]
        settings: JsonValue,
    },
    Companion {
        connection_id: String,
        action_id: String,
        #[serde(default)]
        options: JsonValue,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub kind: BindingKind,
}

impl Binding {
    pub fn stream_deck(
        plugin_uuid: impl Into<String>,
        action_uuid: impl Into<String>,
        instance_id: ActionInstanceId,
        settings: JsonValue,
    ) -> Self {
        Self {
            kind: BindingKind::StreamDeck {
                plugin_uuid: plugin_uuid.into(),
                action_uuid: action_uuid.into(),
                instance_id,
                settings,
            },
        }
    }

    pub fn companion(
        connection_id: impl Into<String>,
        action_id: impl Into<String>,
        options: JsonValue,
    ) -> Self {
        Self {
            kind: BindingKind::Companion {
                connection_id: connection_id.into(),
                action_id: action_id.into(),
                options,
            },
        }
    }
}
