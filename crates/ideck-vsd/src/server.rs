//! Minimal VSD2 WebSocket server for integratedeck-owned software surfaces.
//!
//! The stock Stream Deck app does not expose a pairing UI for external
//! clients, so this server owns its own trust model: any client that completes
//! the Hello exchange is accepted. Virtual devices are in-memory and exposed
//! through the same protobuf messages as the official server.

use std::net::SocketAddr;
use std::sync::Arc;

use crypto_box::PublicKey;
use futures::{SinkExt, StreamExt};
use prost::Message;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{accept_async, WebSocketStream};

use crate::messages::{
    client_message, server_message::Msg as ServerMsg, ButtonImage, ClientMessage, KeypadConfig, Ok,
    ServerMessage, VirtualDevice, VirtualDeviceList,
};
use crate::VsdCrypto;

type Sink = futures::stream::SplitSink<WebSocketStream<TcpStream>, WsMessage>;
type Stream = futures::stream::SplitStream<WebSocketStream<TcpStream>>;

#[derive(Debug, Clone)]
pub struct VsdServerConfig {
    pub hostname: String,
    pub workstation_id: String,
    pub rows: u32,
    pub columns: u32,
}

impl Default for VsdServerConfig {
    fn default() -> Self {
        Self {
            hostname: "integratedeck".into(),
            workstation_id: format!("ideck-{}", std::process::id()),
            rows: 3,
            columns: 5,
        }
    }
}

pub struct VsdServer {
    listener: TcpListener,
    config: VsdServerConfig,
    devices: Arc<Mutex<Vec<VirtualDevice>>>,
    events: broadcast::Sender<VsdServerEvent>,
    images: broadcast::Sender<VsdImage>,
}

/// A rendered key image for one virtual device.
#[derive(Debug, Clone)]
pub struct VsdImage {
    pub device_id: String,
    pub index: u32,
    pub jpeg: Vec<u8>,
}

/// Server-side notifications for the application layer.
#[derive(Debug, Clone)]
pub enum VsdServerEvent {
    DeviceOpened {
        device_id: String,
        name: String,
        rows: u32,
        columns: u32,
    },
    KeyEvent {
        device_id: String,
        index: u32,
        pressed: bool,
    },
}

impl VsdServer {
    pub async fn bind(addr: &str, config: VsdServerConfig) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        let devices = Arc::new(Mutex::new(vec![VirtualDevice {
            id: "integratedeck-primary".into(),
            name: format!("integratedeck {}x{}", config.rows, config.columns),
        }]));
        let (events, _) = broadcast::channel(64);
        let (images, _) = broadcast::channel(128);
        Ok(Self {
            listener,
            config,
            devices,
            events,
            images,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<VsdServerEvent> {
        self.events.subscribe()
    }

    pub fn image_sender(&self) -> broadcast::Sender<VsdImage> {
        self.images.clone()
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub async fn run(self) {
        loop {
            match self.listener.accept().await {
                Ok((stream, _)) => {
                    let config = self.config.clone();
                    let devices = self.devices.clone();
                    let events = self.events.clone();
                    let images = self.images.subscribe();
                    tokio::spawn(async move {
                        if let Err(error) =
                            handle_connection(stream, config, devices, events, images).await
                        {
                            tracing::debug!("vsd connection ended: {error:#}");
                        }
                    });
                }
                Err(error) => {
                    tracing::warn!("vsd accept failed: {error}");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    config: VsdServerConfig,
    devices: Arc<Mutex<Vec<VirtualDevice>>>,
    events: broadcast::Sender<VsdServerEvent>,
    mut images: broadcast::Receiver<VsdImage>,
) -> anyhow::Result<()> {
    let websocket = accept_async(stream).await?;
    let (mut sink, mut stream) = websocket.split();

    let hello_bytes = next_binary(&mut stream)
        .await?
        .ok_or_else(|| anyhow::anyhow!("client closed before Hello"))?;
    let hello = ClientMessage::decode(hello_bytes.as_slice())?;
    let client_key = match hello.msg {
        Some(client_message::Msg::Hello(hello)) => hello.public_key,
        _ => anyhow::bail!("expected HelloFromClient"),
    };
    let client_public = PublicKey::from_slice(&client_key)
        .map_err(|_| anyhow::anyhow!("invalid client public key"))?;
    let (host_secret, host_public) = VsdCrypto::generate_keypair();
    let crypto = VsdCrypto::new(&host_secret, &client_public);

    send_plain(
        &mut sink,
        &ServerMessage {
            msg: Some(ServerMsg::Hello(crate::HelloFromServer {
                workstation_id: config.workstation_id.clone(),
                hostname: config.hostname.clone(),
                version: "7.6.0".into(),
                os_type: 3,
                public_key: host_public.as_bytes().to_vec(),
                client_needs_authentication: false,
                os_name: std::env::consts::OS.into(),
                os_version: String::new(),
                service_endpoints: None,
            })),
        },
    )
    .await?;
    send_encrypted(
        &mut sink,
        &crypto,
        &ServerMessage {
            msg: Some(ServerMsg::KeypadConfig(KeypadConfig {
                rows: config.rows,
                columns: config.columns,
            })),
        },
    )
    .await?;

    let (frame_tx, mut frames) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    let mut reader = stream;
    tokio::spawn(async move {
        while let Ok(Some(frame)) = next_binary(&mut reader).await {
            if frame_tx.send(frame).await.is_err() {
                break;
            }
        }
    });

    let mut opened_device_id: Option<String> = None;
    loop {
        tokio::select! {
            frame = frames.recv() => {
                let Some(frame) = frame else { break };
                let plaintext = crypto
                    .decrypt(&frame)
                    .map_err(|error| anyhow::anyhow!("decrypt: {error}"))?;
                let message = ClientMessage::decode(plaintext.as_slice())?;
                match message.msg {
                    Some(client_message::Msg::Authenticate(_)) => {
                        send_ok(&mut sink, &crypto).await?;
                    }
                    Some(client_message::Msg::CreateVirtualDevice(create)) => {
                        let id = format!("ideck-{}", uuid_like(devices.lock().await.len() + 1));
                        let name = create.name.clone();
                        devices.lock().await.push(VirtualDevice {
                            id: id.clone(),
                            name: name.clone(),
                        });
                        opened_device_id = Some(id.clone());
                        send_ok(&mut sink, &crypto).await?;
                        send_encrypted(
                            &mut sink,
                            &crypto,
                            &ServerMessage {
                                msg: Some(ServerMsg::VirtualDevices(VirtualDeviceList {
                                    devices: devices.lock().await.clone(),
                                })),
                            },
                        )
                        .await?;
                        let _ = events.send(VsdServerEvent::DeviceOpened {
                            device_id: id,
                            name,
                            rows: config.rows,
                            columns: config.columns,
                        });
                    }
                    Some(client_message::Msg::OpenVirtualDevice(open)) => {
                        opened_device_id = Some(open.device_id.clone());
                        send_ok(&mut sink, &crypto).await?;
                        let _ = events.send(VsdServerEvent::DeviceOpened {
                            device_id: open.device_id,
                            name: "virtual device".into(),
                            rows: config.rows,
                            columns: config.columns,
                        });
                    }
                    Some(client_message::Msg::KeyEvent(event)) => {
                        if let Some(device_id) = &opened_device_id {
                            let _ = events.send(VsdServerEvent::KeyEvent {
                                device_id: device_id.clone(),
                                index: event.index,
                                pressed: event.pressed,
                            });
                        }
                        tracing::debug!(index = event.index, pressed = event.pressed, "vsd key event");
                    }
                    _ => {}
                }
            }
            image = images.recv() => {
                let Ok(image) = image else { continue };
                if opened_device_id.as_deref() == Some(image.device_id.as_str()) {
                    send_encrypted(
                        &mut sink,
                        &crypto,
                        &ServerMessage {
                            msg: Some(ServerMsg::ButtonImage(ButtonImage {
                                index: image.index,
                                jpeg: image.jpeg,
                            })),
                        },
                    )
                    .await?;
                }
            }
        }
    }
    Ok(())
}

fn uuid_like(seed: usize) -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    nanos ^ (seed as u64) << 32
}

async fn send_plain(sink: &mut Sink, message: &ServerMessage) -> anyhow::Result<()> {
    sink.send(WsMessage::Binary(message.encode_to_vec().into()))
        .await?;
    Ok(())
}

async fn send_encrypted(
    sink: &mut Sink,
    crypto: &VsdCrypto,
    message: &ServerMessage,
) -> anyhow::Result<()> {
    let frame = crypto.encrypt(&message.encode_to_vec())?;
    sink.send(WsMessage::Binary(frame.into())).await?;
    Ok(())
}

async fn send_ok(sink: &mut Sink, crypto: &VsdCrypto) -> anyhow::Result<()> {
    send_encrypted(
        sink,
        crypto,
        &ServerMessage {
            msg: Some(ServerMsg::Ok(Ok {})),
        },
    )
    .await
}

async fn next_binary(stream: &mut Stream) -> anyhow::Result<Option<Vec<u8>>> {
    loop {
        let Some(message) = stream.next().await else {
            return Ok(None);
        };
        match message? {
            WsMessage::Binary(bytes) => return Ok(Some(bytes.to_vec())),
            WsMessage::Close(_) => return Ok(None),
            _ => {}
        }
    }
}
