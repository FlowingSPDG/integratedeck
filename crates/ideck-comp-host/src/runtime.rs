//! Orchestrator-managed Node companion module host (stdio IPC).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Serialize)]
pub struct HostRequest {
    pub id: String,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostResponse {
    pub id: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct HostEvent {
    #[serde(default)]
    pub event: String,
    #[serde(flatten)]
    pub data: Value,
}

enum HostLine {
    Response(HostResponse),
    Event(HostEvent),
}

/// Manages the shared Node companion host process (one per app, not a sidecar daemon).
pub struct CompModuleRuntime {
    request_tx: mpsc::UnboundedSender<HostRequest>,
    pending: Arc<RwLock<HashMap<String, oneshot::Sender<HostResponse>>>>,
    event_tx: broadcast::Sender<HostEvent>,
    _child: Arc<Mutex<Option<Child>>>,
}

use tokio::sync::broadcast;

impl CompModuleRuntime {
    pub async fn spawn(host_script: &Path, node: &Path) -> anyhow::Result<Self> {
        if !host_script.exists() {
            anyhow::bail!("companion host script not found: {}", host_script.display());
        }

        let mut child = Command::new(node)
            .arg(host_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let mut stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (req_tx, mut req_rx) = mpsc::unbounded_channel::<HostRequest>();
        let pending: Arc<RwLock<HashMap<String, oneshot::Sender<HostResponse>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let (event_tx, _) = broadcast::channel(128);
        let child_arc = Arc::new(Mutex::new(Some(child)));

        let pending_reader = pending.clone();
        let event_tx_reader = event_tx.clone();
        let child_reader = child_arc.clone();

        let mut lines = BufReader::new(stdout).lines();
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                match parse_host_line(&line) {
                    Some(HostLine::Response(resp)) => {
                        if let Some(tx) = pending_reader.write().await.remove(&resp.id) {
                            let _ = tx.send(resp);
                        }
                    }
                    Some(HostLine::Event(ev)) => {
                        let _ = event_tx_reader.send(ev);
                    }
                    None => {}
                }
            }
            warn!("companion host stdout closed");
            child_reader.lock().await.take();
        });

        tokio::spawn(async move {
            while let Some(req) = req_rx.recv().await {
                let line = match serde_json::to_string(&req) {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.write_all(b"\n").await.is_err() {
                    break;
                }
            }
        });

        info!("companion module host started");
        Ok(Self {
            request_tx: req_tx,
            pending,
            event_tx,
            _child: child_arc,
        })
    }

    pub fn host_script_path() -> PathBuf {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../companion-host/dist/index.js");
        if dev.exists() {
            return dev;
        }
        PathBuf::from("companion-host/dist/index.js")
    }

    pub async fn request(
        &self,
        method: impl Into<String>,
        params: Value,
    ) -> anyhow::Result<HostResponse> {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.write().await.insert(id.clone(), tx);
        self.request_tx.send(HostRequest {
            id: id.clone(),
            method: method.into(),
            params,
        })?;
        rx.await.map_err(|_| anyhow::anyhow!("host response channel closed"))
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<HostEvent> {
        self.event_tx.subscribe()
    }

    pub async fn ping(&self) -> anyhow::Result<Value> {
        let resp = self.request("ping", Value::Null).await?;
        if resp.ok {
            Ok(resp.result.unwrap_or(Value::Null))
        } else {
            anyhow::bail!(resp.error.unwrap_or_else(|| "ping failed".into()))
        }
    }
}

fn parse_host_line(line: &str) -> Option<HostLine> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("event").is_some() && value.get("id").is_none() {
        return Some(HostLine::Event(serde_json::from_value(value).ok()?));
    }
    if value.get("id").is_some() {
        return Some(HostLine::Response(serde_json::from_value(value).ok()?));
    }
    None
}
