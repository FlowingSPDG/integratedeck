//! Companion module host backed by a Node sidecar.
//!
//! Real Companion modules extend `@companion-module/base` and use Node APIs
//! (`net`, `http`, `companion-module-utils`). The embedded Boa runtime cannot
//! provide those, so the sidecar runs the module under Node while keeping the
//! same request/event surface as the previous embedded engine.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, oneshot, Mutex};

use crate::runtime::{HostEvent, HostResponse};

const SIDECAR: &str = include_str!("../assets/companion_sidecar.mjs");
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

pub struct EmbeddedEngine {
    stdin: Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<HostResponse>>>>,
    event_tx: broadcast::Sender<HostEvent>,
    _child: Mutex<Child>,
}

impl EmbeddedEngine {
    pub fn spawn(modules_dir: &Path) -> anyhow::Result<Self> {
        let script_path = std::env::temp_dir().join(format!(
            "ideck-companion-sidecar-{}.mjs",
            std::process::id()
        ));
        std::fs::write(&script_path, SIDECAR)?;

        let mut child = Command::new(node_binary())
            .arg(&script_path)
            .arg(modules_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to start Node companion sidecar: {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("companion sidecar stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("companion sidecar stdout unavailable"))?;

        let (event_tx, _) = broadcast::channel(256);
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<HostResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_reader = pending.clone();
        let event_reader = event_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if let Some(id) = value.get("id").and_then(Value::as_str) {
                    let response = HostResponse {
                        id: id.to_string(),
                        ok: value.get("ok").and_then(Value::as_bool).unwrap_or(false),
                        result: value.get("result").cloned(),
                        error: value
                            .get("error")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    };
                    if let Some(tx) = pending_reader.lock().await.remove(id) {
                        let _ = tx.send(response);
                    }
                    continue;
                }
                let event = value
                    .get("event")
                    .and_then(Value::as_str)
                    .unwrap_or("message")
                    .to_string();
                let _ = event_reader.send(HostEvent { event, data: value });
            }
        });

        Ok(Self {
            stdin: Mutex::new(stdin),
            pending,
            event_tx,
            _child: Mutex::new(child),
        })
    }

    pub async fn request(
        &self,
        method: impl Into<String>,
        params: Value,
    ) -> anyhow::Result<HostResponse> {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);

        let request = json!({
            "id": id,
            "method": method.into(),
            "params": params,
        });
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(format!("{request}\n").as_bytes()).await?;
        stdin.flush().await?;
        drop(stdin);

        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => anyhow::bail!("companion sidecar closed before responding"),
            Err(_) => anyhow::bail!("companion sidecar request timed out"),
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<HostEvent> {
        self.event_tx.subscribe()
    }

    pub async fn ping(&self) -> anyhow::Result<Value> {
        let response = self.request("ping", Value::Null).await?;
        if response.ok {
            Ok(response.result.unwrap_or(Value::Null))
        } else {
            anyhow::bail!(response.error.unwrap_or_else(|| "ping failed".into()))
        }
    }
}

fn node_binary() -> String {
    std::env::var("IDECK_NODE").unwrap_or_else(|_| "node".to_string())
}
