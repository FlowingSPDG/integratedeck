use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{ActionInstanceId, PageId};

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
    /// Built-in navigation actions (folder, back, page switch).
    BuiltIn {
        action_id: String,
        #[serde(default)]
        settings: JsonValue,
    },
    /// Executes multiple bindings in sequence (Stream Deck Multi Action).
    MultiAction {
        #[serde(default)]
        steps: Vec<MultiActionStep>,
        /// Delay between steps in milliseconds (Stream Deck default: 200).
        #[serde(default = "default_multi_delay_ms")]
        delay_ms: u32,
    },
}

fn default_multi_delay_ms() -> u32 {
    200
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiActionStep {
    pub binding: BindingKind,
    /// Optional delay before this step runs (e.g. Stream Deck Delay action).
    #[serde(default)]
    pub delay_before_ms: u32,
}

/// Parsed settings for built-in folder navigation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSettings {
    #[serde(default, alias = "ProfileUUID", alias = "profile_uuid")]
    pub child_page_id: Option<String>,
}

impl FolderSettings {
    pub fn child_page_id(&self) -> Option<PageId> {
        let raw = self.child_page_id.as_ref()?;
        uuid::Uuid::parse_str(raw).ok().map(PageId)
    }
}

/// Parsed settings for built-in page / profile switch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchPageSettings {
    #[serde(default, alias = "ProfileUUID", alias = "profile_uuid")]
    pub target_page_id: Option<String>,
    #[serde(default)]
    pub target_page_name: Option<String>,
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

    pub fn built_in(action_id: impl Into<String>, settings: JsonValue) -> Self {
        Self {
            kind: BindingKind::BuiltIn {
                action_id: action_id.into(),
                settings,
            },
        }
    }

    pub fn multi_action(steps: Vec<MultiActionStep>, delay_ms: u32) -> Self {
        Self {
            kind: BindingKind::MultiAction { steps, delay_ms },
        }
    }
}
