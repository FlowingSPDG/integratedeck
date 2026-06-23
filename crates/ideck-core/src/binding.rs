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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Binding {
    #[serde(flatten)]
    pub kind: BindingKind,
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = JsonValue::deserialize(deserializer)?;
        if let Ok(kind) = serde_json::from_value::<BindingKind>(value.clone()) {
            return Ok(Binding { kind });
        }
        if let Some(inner) = value.get("kind") {
            if let Ok(kind) = serde_json::from_value::<BindingKind>(inner.clone()) {
                return Ok(Binding { kind });
            }
        }
        Err(serde::de::Error::custom("invalid binding"))
    }
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
