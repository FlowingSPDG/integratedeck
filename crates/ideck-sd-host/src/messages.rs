use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Inbound message from plugin or property inspector.
#[derive(Debug, Clone, Deserialize)]
pub struct InboundMessage {
    pub event: String,
    #[serde(flatten)]
    pub rest: Value,
}

/// Outbound event to plugin (subset of Stream Deck SDK).
#[derive(Debug, Clone, Serialize)]
pub struct OutboundEvent {
    pub event: String,
    #[serde(flatten)]
    pub payload: Value,
}

impl OutboundEvent {
    pub fn key_down(context: Value) -> Self {
        Self {
            event: "keyDown".into(),
            payload: serde_json::json!({ "action": "", "context": context, "device": "", "payload": { "settings": {}, "coordinates": { "column": 0, "row": 0 }, "isInMultiAction": false }, "controller": "Keypad" }),
        }
    }

    pub fn will_appear(context: Value, settings: Value) -> Self {
        Self {
            event: "willAppear".into(),
            payload: serde_json::json!({
                "action": "",
                "context": context,
                "device": "",
                "payload": {
                    "settings": settings,
                    "coordinates": { "column": 0, "row": 0 },
                    "isInMultiAction": false
                }
            }),
        }
    }

    pub fn did_receive_settings(context: Value, settings: Value) -> Self {
        Self {
            event: "didReceiveSettings".into(),
            payload: serde_json::json!({
                "action": "",
                "context": context,
                "device": "",
                "payload": { "settings": settings }
            }),
        }
    }
}

/// Command from plugin to host (parsed subset).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum PluginCommand {
    #[serde(rename = "setImage")]
    SetImage {
        context: String,
        payload: SetImagePayload,
    },
    #[serde(rename = "setTitle")]
    SetTitle {
        context: String,
        payload: SetTitlePayload,
    },
    #[serde(rename = "setSettings")]
    SetSettings {
        context: String,
        payload: SettingsPayload,
    },
    #[serde(rename = "getSettings")]
    GetSettings { context: String },
    #[serde(rename = "sendToPropertyInspector")]
    SendToPropertyInspector {
        context: String,
        payload: Value,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetImagePayload {
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub target: Option<u8>,
    #[serde(rename = "state", default)]
    pub state: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetTitlePayload {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub target: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingsPayload {
    pub settings: Value,
}
