use std::path::Path;
use std::sync::Arc;

use ideck_js_runtime::{JsEngine, JsEngineConfig};
use serde_json::Value as JsonValue;
use tokio::sync::broadcast;

use crate::runtime::{HostEvent, HostResponse};

const BOOTSTRAP: &str = include_str!("../assets/compat_bootstrap.js");

pub struct EmbeddedEngine {
    inner: JsEngine,
    modules_root: std::path::PathBuf,
    event_tx: broadcast::Sender<HostEvent>,
}

impl EmbeddedEngine {
    pub fn spawn(modules_dir: &Path) -> anyhow::Result<Self> {
        let (event_tx, _) = broadcast::channel(128);
        let event_tx_clone = event_tx.clone();
        let emitter: Arc<dyn Fn(String, String) + Send + Sync> = Arc::new(move |event, data_json| {
            let mut value: JsonValue =
                serde_json::from_str(&data_json).unwrap_or_else(|_| serde_json::json!({}));
            if let JsonValue::Object(ref mut map) = value {
                map.insert("event".into(), JsonValue::String(event.clone()));
                let host_event = HostEvent {
                    event,
                    data: JsonValue::Object(map.clone()),
                };
                let _ = event_tx_clone.send(host_event);
            }
        });

        let modules_root = modules_dir.to_path_buf();
        let inner = JsEngine::spawn(JsEngineConfig {
            module_root: modules_root.clone(),
            bootstrap: BOOTSTRAP,
            thread_name: "ideck-comp-js",
            with_websocket: false,
            event_emitter: Some(emitter),
            startup_call: None,
        })?;

        let _ = event_tx.send(HostEvent {
            event: "ready".into(),
            data: serde_json::json!({
                "version": "1.0.0",
                "engine": "ideck-comp-host-boa"
            }),
        });

        Ok(Self {
            inner,
            modules_root,
            event_tx,
        })
    }

    pub async fn request(
        &self,
        method: impl Into<String>,
        params: JsonValue,
    ) -> anyhow::Result<HostResponse> {
        let id = uuid::Uuid::new_v4().to_string();
        let method = method.into();
        let modules_root = self.modules_root.to_string_lossy().to_string();
        let call_params = serde_json::json!([method, params, modules_root]);

        match self.inner.call("__ideckHandle", call_params).await {
            Ok(result) => Ok(HostResponse {
                id,
                ok: true,
                result: Some(result),
                error: None,
            }),
            Err(e) => Ok(HostResponse {
                id,
                ok: false,
                result: None,
                error: Some(format!("{e:#}")),
            }),
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<HostEvent> {
        self.event_tx.subscribe()
    }

    pub async fn ping(&self) -> anyhow::Result<JsonValue> {
        let resp = self.request("ping", JsonValue::Null).await?;
        if resp.ok {
            Ok(resp.result.unwrap_or(JsonValue::Null))
        } else {
            anyhow::bail!(resp.error.unwrap_or_else(|| "ping failed".into()))
        }
    }
}
