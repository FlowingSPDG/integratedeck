use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, info, warn};

use crate::{InboundMessage, PluginProcess, StreamDeckManifest};

type WsSink = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<TcpStream>,
    Message,
>;

#[derive(Debug, Clone)]
pub struct ActionContext {
    pub context: String,
    pub action_uuid: String,
    pub settings: Value,
    pub coordinates: (u32, u32),
}

#[derive(Debug, Clone)]
pub enum BrokerEvent {
    PluginRegistered { plugin_uuid: String },
    SetImage {
        context: String,
        image_base64: Option<String>,
        state: u8,
    },
    SetTitle { context: String, title: Option<String> },
    SettingsChanged { context: String, settings: Value },
    SendToPropertyInspector { context: String, payload: Value },
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
    _plugin_process: PluginProcess,
}

impl StreamDeckBroker {
    pub async fn start(
        plugin_dir: &std::path::Path,
        node_binary: &std::path::Path,
    ) -> anyhow::Result<(Arc<Self>, mpsc::UnboundedReceiver<BrokerEvent>)> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let manifest = StreamDeckManifest::load(plugin_dir)?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (plugin_msg_tx, mut plugin_msg_rx) = mpsc::unbounded_channel();

        info!("starting SD plugin on port {port}");
        let plugin_process = PluginProcess::spawn_node(plugin_dir, port, node_binary).await?;

        let broker = Arc::new(Self {
            port,
            plugin_uuid: manifest.uuid.clone(),
            plugin_tx: plugin_msg_tx,
            pi_connections: RwLock::new(HashMap::new()),
            plugin_sink: RwLock::new(None),
            pending_plugin_msgs: Mutex::new(VecDeque::new()),
            contexts: RwLock::new(HashMap::new()),
            event_tx,
            device_id: "integratedeck-virtual-1".into(),
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
        let mut payload = json!({
            "action": ctx.action_uuid,
            "context": context,
            "device": self.device_id,
            "payload": {
                "settings": ctx.settings,
                "coordinates": { "column": ctx.coordinates.0, "row": ctx.coordinates.1 },
                "isInMultiAction": false
            },
            "controller": "Keypad"
        });
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("event".into(), json!(event));
        }
        self.plugin_tx
            .send(Message::Text(payload.to_string().into()))?;
        Ok(())
    }

    pub async fn will_appear(&self, context: &str) -> anyhow::Result<()> {
        let ctx = self
            .contexts
            .read()
            .await
            .get(context)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown context"))?;
        let mut payload = json!({
            "action": ctx.action_uuid,
            "context": context,
            "device": self.device_id,
            "payload": {
                "settings": ctx.settings,
                "coordinates": { "column": ctx.coordinates.0, "row": ctx.coordinates.1 },
                "isInMultiAction": false
            }
        });
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("event".into(), json!("willAppear"));
        }
        self.plugin_tx
            .send(Message::Text(payload.to_string().into()))?;
        Ok(())
    }

    async fn handle_plugin_message(&self, text: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(text) else {
            return;
        };
        let event = msg.get("event").and_then(|v| v.as_str()).unwrap_or("");
        match event {
            "setImage" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                let image = msg
                    .pointer("/payload/image")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let state = msg
                    .pointer("/payload/state")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u8;
                let _ = self.event_tx.send(BrokerEvent::SetImage {
                    context: context.into(),
                    image_base64: image,
                    state,
                });
            }
            "setTitle" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                let title = msg
                    .pointer("/payload/title")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let _ = self.event_tx.send(BrokerEvent::SetTitle {
                    context: context.into(),
                    title,
                });
            }
            "setSettings" => {
                let context = msg.get("context").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(settings) = msg.pointer("/payload/settings").cloned() {
                    if let Some(ctx) = self.contexts.write().await.get_mut(context) {
                        ctx.settings = settings.clone();
                    }
                    let _ = self.event_tx.send(BrokerEvent::SettingsChanged {
                        context: context.into(),
                        settings,
                    });
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
                        "payload": { "settings": ctx.settings }
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
                let out = json!({ "event": "sendToPropertyInspector", "payload": payload });
                let mut pi = self.pi_connections.write().await;
                if let Some(sink) = pi.get_mut(context) {
                    let _ = sink
                        .send(Message::Text(out.to_string().into()))
                        .await;
                }
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
    if matches!(event, "setSettings" | "sendToPlugin") {
        let _ = broker.plugin_tx.send(Message::Text(text.into()));
    }
}
