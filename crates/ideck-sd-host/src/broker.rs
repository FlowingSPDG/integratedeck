use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, info, warn};

use ideck_core::{image_from_plugin_payload, ImagePayload, TitleParams};

use crate::{InboundMessage, PluginProcess, StreamDeckManifest};

type WsSink = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<TcpStream>,
    Message,
>;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub rows: u32,
    pub columns: u32,
    pub device_type: i32,
}

#[derive(Debug, Clone)]
pub enum BrokerEvent {
    PluginRegistered { plugin_uuid: String },
    SetImage {
        context: String,
        /// `Unchanged` = payload omitted `image`; `Cleared` = null/empty; `Set` = new bytes.
        image: ImageSetResult,
        state: u8,
    },
    SetTitle {
        context: String,
        title: Option<String>,
        title_params: Option<TitleParams>,
    },
    SettingsChanged { context: String, settings: Value },
    SendToPropertyInspector { context: String, payload: Value },
    ShowAlert { context: String },
    ShowOk { context: String },
    SetState { context: String, state: u8 },
    OpenUrl { url: String },
    LogMessage { message: String },
}

/// Stream Deck-compatible WebSocket broker (dynamic port per plugin session).
pub struct StreamDeckBroker {
    port: u16,
    plugin_uuid: String,
    plugin_tx: mpsc::UnboundedSender<Message>,
    pi_connections: RwLock<HashMap<String, WsSink>>,
    plugin_sink: RwLock<Option<WsSink>>,
    pending_plugin_msgs: Mutex<VecDeque<Message>>,
    contexts: RwLock<HashMap<String, ActionContext>>,
    event_tx: mpsc::UnboundedSender<BrokerEvent>,
    device_id: String,
    device_info: DeviceInfo,
    global_settings: RwLock<Value>,
    plugin_dir: PathBuf,
    _plugin_process: PluginProcess,
}

#[derive(Debug, Clone)]
pub enum ImageSetResult {
    Unchanged,
    Cleared,
    Set(ImagePayload),
}

#[derive(Debug, Clone)]
pub struct ActionContext {
    pub context: String,
    pub action_uuid: String,
    pub settings: Value,
    pub coordinates: (u32, u32),
}

impl StreamDeckBroker {
    pub async fn start(
        plugin_dir: &std::path::Path,
        devices: Value,
    ) -> anyhow::Result<(Arc<Self>, mpsc::UnboundedReceiver<BrokerEvent>)> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let manifest = StreamDeckManifest::load(plugin_dir)?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (plugin_msg_tx, mut plugin_msg_rx) = mpsc::unbounded_channel();

        info!("starting SD plugin on port {port}");
        let plugin_process = PluginProcess::spawn(plugin_dir, port, devices.clone()).await?;

        let device_id = devices
            .as_array()
            .and_then(|a| a.first())
            .and_then(|d| d.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("integratedeck-device-1")
            .to_string();

        let device_info = parse_device_info(&devices, &device_id);

        let broker = Arc::new(Self {
            port,
            plugin_uuid: manifest.uuid.clone(),
            plugin_tx: plugin_msg_tx,
            pi_connections: RwLock::new(HashMap::new()),
            plugin_sink: RwLock::new(None),
            pending_plugin_msgs: Mutex::new(VecDeque::new()),
            contexts: RwLock::new(HashMap::new()),
            event_tx,
            device_id,
            device_info,
            global_settings: RwLock::new(json!({})),
            plugin_dir: plugin_dir.to_path_buf(),
            _plugin_process: plugin_process,
        });

        let broker_accept = broker.clone();
        tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                let b = broker_accept.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(b, stream, addr).await {
                        warn!("ws connection error: {e}");
                    }
                });
            }
        });

        let broker_forward = broker.clone();
        tokio::spawn(async move {
            while let Some(msg) = plugin_msg_rx.recv().await {
                broker_forward.enqueue_or_send_to_plugin(msg).await;
            }
        });

        Ok((broker, event_rx))
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn plugin_uuid(&self) -> &str {
        &self.plugin_uuid
    }

    pub async fn register_context(&self, ctx: ActionContext) {
        self.contexts
            .write()
            .await
            .insert(ctx.context.clone(), ctx);
    }

    pub async fn key_down(&self, context: &str) -> anyhow::Result<()> {
        self.send_key_event(context, "keyDown").await
    }

    pub async fn key_up(&self, context: &str) -> anyhow::Result<()> {
        self.send_key_event(context, "keyUp").await
    }

    async fn send_key_event(&self, context: &str, event: &str) -> anyhow::Result<()> {
        let ctx = self
            .contexts
            .read()
            .await
            .get(context)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown context"))?;
        let payload = json!({
            "event": event,
            "action": ctx.action_uuid,
            "context": context,
            "device": self.device_id,
            "payload": single_action_payload(&ctx)
        });
        self.plugin_tx
            .send(Message::Text(payload.to_string().into()))?;
        Ok(())
    }

    pub async fn will_disappear(&self, context: &str) -> anyhow::Result<()> {
        self.send_lifecycle(context, "willDisappear").await
    }

    pub async fn property_inspector_did_appear(&self, context: &str) -> anyhow::Result<()> {
        self.send_simple_plugin_event(context, "propertyInspectorDidAppear")
            .await?;
        send_did_receive_settings_to_pi(self, context).await;
        Ok(())
    }

    pub async fn property_inspector_did_disappear(&self, context: &str) -> anyhow::Result<()> {
        self.send_simple_plugin_event(context, "propertyInspectorDidDisappear")
            .await
    }

    pub async fn device_did_connect(&self) -> anyhow::Result<()> {
        let info = &self.device_info;
        let payload = json!({
            "event": "deviceDidConnect",
            "device": self.device_id,
            "deviceInfo": {
                "name": info.name,
                "type": info.device_type,
                "size": { "columns": info.columns, "rows": info.rows }
            }
        });
        self.plugin_tx
            .send(Message::Text(payload.to_string().into()))?;
        Ok(())
    }

    async fn send_simple_plugin_event(&self, context: &str, event: &str) -> anyhow::Result<()> {
        let ctx = self
            .contexts
            .read()
            .await
            .get(context)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown context"))?;
        let payload = json!({
            "event": event,
            "action": ctx.action_uuid,
            "context": context,
            "device": self.device_id
        });
        self.plugin_tx
            .send(Message::Text(payload.to_string().into()))?;
        Ok(())
    }

    async fn send_lifecycle(&self, context: &str, event: &str) -> anyhow::Result<()> {
        let ctx = self
            .contexts
            .read()
            .await
            .get(context)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown context"))?;
        let payload = json!({
            "event": event,
            "action": ctx.action_uuid,
            "context": context,
            "device": self.device_id,
            "payload": single_action_payload(&ctx)
        });
        self.plugin_tx
            .send(Message::Text(payload.to_string().into()))?;
        Ok(())
    }

    pub async fn dial_rotate(
        &self,
        context: &str,
        ticks: i32,
        pressed: bool,
    ) -> anyhow::Result<()> {
        let ctx = self
            .contexts
            .read()
            .await
            .get(context)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown context"))?;
        let payload = json!({
            "event": "dialRotate",
            "action": ctx.action_uuid,
            "context": context,
            "device": self.device_id,
            "payload": {
                "settings": ctx.settings,
                "coordinates": { "column": ctx.coordinates.0, "row": ctx.coordinates.1 },
                "isInMultiAction": false,
                "controller": "Encoder",
                "resources": {},
                "ticks": ticks,
                "pressed": pressed
            }
        });
        self.plugin_tx
            .send(Message::Text(payload.to_string().into()))?;
        Ok(())
    }

    pub async fn will_appear(&self, context: &str) -> anyhow::Result<()> {
        self.send_lifecycle(context, "willAppear").await
    }

    async fn handle_plugin_message(&self, text: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(text) else {
            return;
        };
        let event = msg.get("event").and_then(|v| v.as_str()).unwrap_or("");
        match event {
            "setImage" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                let image_field = msg.pointer("/payload/image");
                let image = match image_field {
                    None => ImageSetResult::Unchanged,
                    Some(v) if v.is_null() => ImageSetResult::Cleared,
                    Some(v) => match v.as_str() {
                        None => ImageSetResult::Unchanged,
                        Some(s) if s.is_empty() => ImageSetResult::Cleared,
                        Some(s) => match image_from_plugin_payload(s, Some(&self.plugin_dir)) {
                            Some(payload) => ImageSetResult::Set(payload),
                            None => {
                                warn!("setImage failed to decode image for context {context}: {s}");
                                ImageSetResult::Unchanged
                            }
                        },
                    },
                };
                let state = msg
                    .pointer("/payload/state")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u8;
                debug!("setImage context={context} image={image:?} state={state}");
                let _ = self.event_tx.send(BrokerEvent::SetImage {
                    context: context.into(),
                    image,
                    state,
                });
            }
            "setTitle" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                let title = msg.get("payload").and_then(|p| p.get("title")).and_then(|v| {
                    if v.is_null() {
                        None
                    } else {
                        v.as_str().map(String::from)
                    }
                });
                let title_params = msg
                    .pointer("/payload/titleParams")
                    .map(ideck_core::title_params_from_json);
                debug!("setTitle context={context} title={title:?}");
                let _ = self.event_tx.send(BrokerEvent::SetTitle {
                    context: context.into(),
                    title,
                    title_params,
                });
            }
            "setSettings" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(settings) = extract_set_settings_payload(&msg) {
                    if let Some(ctx) = self.contexts.write().await.get_mut(context) {
                        ctx.settings = settings.clone();
                    }
                    let _ = self.event_tx.send(BrokerEvent::SettingsChanged {
                        context: context.into(),
                        settings: settings.clone(),
                    });
                    send_did_receive_settings_to_pi(self, context).await;
                }
            }
            "getSettings" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(ctx) = self.contexts.read().await.get(context) {
                    let payload = json!({
                        "event": "didReceiveSettings",
                        "action": ctx.action_uuid,
                        "context": context,
                        "device": self.device_id,
                        "payload": action_settings_payload(ctx)
                    });
                    self.enqueue_or_send_to_plugin(Message::Text(payload.to_string().into()))
                        .await;
                }
            }
            "sendToPropertyInspector" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                let payload = msg.get("payload").cloned().unwrap_or(json!({}));
                let _ = self.event_tx.send(BrokerEvent::SendToPropertyInspector {
                    context: context.into(),
                    payload: payload.clone(),
                });
                let out = json!({
                    "event": "sendToPropertyInspector",
                    "action": self.contexts.read().await.get(context).map(|c| c.action_uuid.clone()).unwrap_or_default(),
                    "context": context,
                    "payload": payload
                });
                let mut pi = self.pi_connections.write().await;
                if let Some(key) = pi_connection_key(&pi, context) {
                    if let Some(sink) = pi.get_mut(&key) {
                        let _ = sink
                            .send(Message::Text(out.to_string().into()))
                            .await;
                    }
                }
            }
            "showAlert" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                let _ = self.event_tx.send(BrokerEvent::ShowAlert {
                    context: context.into(),
                });
            }
            "showOk" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                let _ = self.event_tx.send(BrokerEvent::ShowOk {
                    context: context.into(),
                });
            }
            "setState" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                let state = msg
                    .pointer("/payload/state")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u8;
                let _ = self.event_tx.send(BrokerEvent::SetState {
                    context: context.into(),
                    state,
                });
            }
            "openUrl" => {
                let url = msg
                    .pointer("/payload/url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let _ = self.event_tx.send(BrokerEvent::OpenUrl { url });
            }
            "logMessage" => {
                let message = msg
                    .pointer("/payload/message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let _ = self.event_tx.send(BrokerEvent::LogMessage { message });
            }
            "setGlobalSettings" => {
                if let Some(settings) = extract_set_settings_payload(&msg) {
                    *self.global_settings.write().await = settings.clone();
                    send_did_receive_global_settings_to_pi(self).await;
                    let payload = json!({
                        "event": "didReceiveGlobalSettings",
                        "payload": { "settings": settings }
                    });
                    self.enqueue_or_send_to_plugin(Message::Text(payload.to_string().into()))
                        .await;
                }
            }
            "getGlobalSettings" => {
                let settings = self.global_settings.read().await.clone();
                let payload = json!({
                    "event": "didReceiveGlobalSettings",
                    "payload": { "settings": settings }
                });
                self.enqueue_or_send_to_plugin(Message::Text(payload.to_string().into()))
                    .await;
            }
            _ => debug!("unhandled plugin command: {event}"),
        }
    }

    async fn enqueue_or_send_to_plugin(&self, msg: Message) {
        let mut guard = self.plugin_sink.write().await;
        if let Some(sink) = guard.as_mut() {
            if sink.send(msg.clone()).await.is_err() {
                *guard = None;
                drop(guard);
                self.pending_plugin_msgs.lock().await.push_back(msg);
            }
        } else {
            drop(guard);
            self.pending_plugin_msgs.lock().await.push_back(msg);
        }
    }

    async fn flush_pending_to_plugin(&self) {
        let mut guard = self.plugin_sink.write().await;
        let Some(sink) = guard.as_mut() else {
            return;
        };
        let mut pending = self.pending_plugin_msgs.lock().await;
        while let Some(msg) = pending.pop_front() {
            if sink.send(msg).await.is_err() {
                *guard = None;
                break;
            }
        }
    }

    async fn on_register_plugin(&self, uuid: &str, sink: WsSink) {
        *self.plugin_sink.write().await = Some(sink);
        self.flush_pending_to_plugin().await;
        let _ = self.event_tx.send(BrokerEvent::PluginRegistered {
            plugin_uuid: uuid.into(),
        });
    }

    async fn on_register_pi(&self, uuid: &str, sink: WsSink) {
        self.pi_connections.write().await.insert(uuid.into(), sink);
        let contexts = self.contexts.read().await;
        let context_to_notify = if contexts.contains_key(uuid) {
            uuid.to_string()
        } else if contexts.len() == 1 {
            contexts
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| uuid.to_string())
        } else {
            uuid.to_string()
        };
        send_did_receive_settings_to_pi(self, &context_to_notify).await;
    }
}

async fn handle_connection(
    broker: Arc<StreamDeckBroker>,
    stream: TcpStream,
    addr: SocketAddr,
) -> anyhow::Result<()> {
    let ws = accept_async(stream).await?;
    let (sink, mut stream) = ws.split();
    let first = stream
        .next()
        .await
        .transpose()?
        .and_then(|m| m.into_text().ok());

    let Some(text) = first else {
        return Ok(());
    };

    let msg: InboundMessage = serde_json::from_str(&text)?;
    debug!("registration from {addr}: {}", msg.event);

    match msg.event.as_str() {
        "registerPlugin" => {
            let uuid = msg
                .rest
                .get("uuid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            broker.on_register_plugin(&uuid, sink).await;
            while let Some(Ok(Message::Text(t))) = stream.next().await {
                broker.handle_plugin_message(&t).await;
            }
        }
        "registerPropertyInspector" => {
            let uuid = msg
                .rest
                .get("uuid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            broker.on_register_pi(&uuid, sink).await;
            while let Some(Ok(Message::Text(t))) = stream.next().await {
                handle_pi_message(&broker, &t).await;
            }
        }
        _ => warn!("unknown registration event: {}", msg.event),
    }
    Ok(())
}

async fn handle_pi_message(broker: &StreamDeckBroker, text: &str) {
    let Ok(msg) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let event = msg.get("event").and_then(|v| v.as_str()).unwrap_or("");
    let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");

    match event {
        "setSettings" => {
            if let Some(settings) = extract_set_settings_payload(&msg) {
                if let Some(ctx) = broker.contexts.write().await.get_mut(context) {
                    ctx.settings = settings.clone();
                }
                let _ = broker.event_tx.send(BrokerEvent::SettingsChanged {
                    context: context.into(),
                    settings: settings.clone(),
                });
                send_did_receive_settings_to_pi(broker, context).await;
                if let Some(ctx) = broker.contexts.read().await.get(context) {
                    let payload = json!({
                        "event": "didReceiveSettings",
                        "action": ctx.action_uuid,
                        "context": context,
                        "device": broker.device_id,
                        "payload": action_settings_payload(ctx)
                    });
                    broker
                        .enqueue_or_send_to_plugin(Message::Text(payload.to_string().into()))
                        .await;
                }
            }
        }
        "getSettings" => {
            send_did_receive_settings_to_pi(broker, context).await;
        }
        "getGlobalSettings" => {
            send_did_receive_global_settings_to_pi(broker).await;
        }
        "setGlobalSettings" => {
            if let Some(settings) = extract_set_settings_payload(&msg) {
                *broker.global_settings.write().await = settings.clone();
                send_did_receive_global_settings_to_pi(broker).await;
                let payload = json!({
                    "event": "didReceiveGlobalSettings",
                    "payload": { "settings": settings }
                });
                broker
                    .enqueue_or_send_to_plugin(Message::Text(payload.to_string().into()))
                    .await;
            }
        }
        "sendToPlugin" | "openUrl" | "logMessage" => {
            broker
                .enqueue_or_send_to_plugin(Message::Text(text.into()))
                .await;
        }
        _ => {}
    }
}

async fn send_did_receive_settings_to_pi(broker: &StreamDeckBroker, context: &str) {
    let Some(ctx) = broker.contexts.read().await.get(context).cloned() else {
        return;
    };
    let payload = json!({
        "event": "didReceiveSettings",
        "action": ctx.action_uuid,
        "context": context,
        "device": broker.device_id,
        "payload": action_settings_payload(&ctx)
    });
    let mut pi = broker.pi_connections.write().await;
    if let Some(key) = pi_connection_key(&pi, context) {
        if let Some(sink) = pi.get_mut(&key) {
            let _ = sink
                .send(Message::Text(payload.to_string().into()))
                .await;
        }
    }
}

fn pi_connection_key(pi: &HashMap<String, WsSink>, context: &str) -> Option<String> {
    if pi.contains_key(context) {
        return Some(context.to_string());
    }
    if pi.len() == 1 {
        return pi.keys().next().cloned();
    }
    None
}

fn action_settings_payload(ctx: &ActionContext) -> Value {
    single_action_payload(ctx)
}

fn single_action_payload(ctx: &ActionContext) -> Value {
    json!({
        "settings": ctx.settings,
        "coordinates": { "column": ctx.coordinates.0, "row": ctx.coordinates.1 },
        "isInMultiAction": false,
        "controller": "Keypad",
        "resources": {}
    })
}

async fn send_did_receive_global_settings_to_pi(broker: &StreamDeckBroker) {
    let settings = broker.global_settings.read().await.clone();
    let payload = json!({
        "event": "didReceiveGlobalSettings",
        "payload": { "settings": settings }
    });
    let mut pi = broker.pi_connections.write().await;
    for sink in pi.values_mut() {
        let _ = sink
            .send(Message::Text(payload.to_string().into()))
            .await;
    }
}

/// Parse settings from a `setSettings` / `setGlobalSettings` message.
///
/// Stream Deck SDK v1/v2 PI sends settings as the payload object directly
/// (`{ "event": "setSettings", "payload": { "key": "value" } }`), while some
/// plugin runtimes wrap them under `payload.settings`.
fn extract_set_settings_payload(msg: &Value) -> Option<Value> {
    let payload = msg.get("payload")?;
    match payload {
        Value::Object(map) if map.contains_key("settings") => Some(map["settings"].clone()),
        Value::Object(_) | Value::Array(_) => Some(payload.clone()),
        Value::Null => None,
        _ => Some(payload.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::extract_set_settings_payload;
    use serde_json::json;

    #[test]
    fn extract_settings_from_pi_payload() {
        let msg = json!({
            "event": "setSettings",
            "context": "ctx",
            "payload": { "foo": "bar", "count": 1 }
        });
        let settings = extract_set_settings_payload(&msg).unwrap();
        assert_eq!(settings, json!({ "foo": "bar", "count": 1 }));
    }

    #[test]
    fn extract_settings_from_wrapped_plugin_payload() {
        let msg = json!({
            "event": "setSettings",
            "context": "ctx",
            "payload": { "settings": { "foo": "bar" } }
        });
        let settings = extract_set_settings_payload(&msg).unwrap();
        assert_eq!(settings, json!({ "foo": "bar" }));
    }
}

fn parse_device_info(devices: &Value, device_id: &str) -> DeviceInfo {
    let first = devices.as_array().and_then(|a| a.first());
    let rows = first
        .and_then(|d| d.pointer("/size/rows"))
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as u32;
    let columns = first
        .and_then(|d| d.pointer("/size/columns"))
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as u32;
    DeviceInfo {
        id: device_id.to_string(),
        name: first
            .and_then(|d| d.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("integratedeck")
            .to_string(),
        rows,
        columns,
        device_type: first
            .and_then(|d| d.get("type"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32,
    }
}
