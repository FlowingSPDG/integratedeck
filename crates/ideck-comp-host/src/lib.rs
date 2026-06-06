//! Companion module host — JSON protocol to Node sidecar.

mod protocol;
mod registry;

pub use protocol::*;
pub use registry::*;

use std::process::Stdio;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Manages the Node sidecar process for Companion modules and surfaces.
pub struct SidecarClient {
    request_tx: mpsc::UnboundedSender<SidecarRequest>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SidecarRequest {
    pub id: String,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SidecarResponse {
    pub id: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

impl SidecarClient {
    pub async fn spawn(
        sidecar_dir: &std::path::Path,
        node: &std::path::Path,
    ) -> anyhow::Result<(Self, mpsc::UnboundedReceiver<SidecarResponse>)> {
        let entry = sidecar_dir.join("dist/index.js");
        let mut child = Command::new(node)
            .arg(&entry)
            .current_dir(sidecar_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let mut stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (req_tx, mut req_rx) = mpsc::unbounded_channel::<SidecarRequest>();
        let (resp_tx, resp_rx) = mpsc::unbounded_channel::<SidecarResponse>();

        let mut lines = BufReader::new(stdout).lines();
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(resp) = serde_json::from_str::<SidecarResponse>(&line) {
                    let _ = resp_tx.send(resp);
                }
            }
            let _ = child.wait().await;
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

        Ok((Self { request_tx: req_tx }, resp_rx))
    }

    pub fn request(&self, method: impl Into<String>, params: Value) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let _ = self.request_tx.send(SidecarRequest {
            id: id.clone(),
            method: method.into(),
            params,
        });
        id
    }

    pub fn ping(&self) -> String {
        self.request("ping", serde_json::json!({}))
    }
}

/// Registry of Companion module connections (metadata only; execution in sidecar).
#[derive(Default)]
pub struct ConnectionRegistry {
    connections: std::collections::HashMap<String, ConnectionRecord>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectionRecord {
    pub id: String,
    pub module_id: String,
    pub label: String,
    #[serde(default)]
    pub config: Value,
    pub enabled: bool,
}

impl ConnectionRegistry {
    pub fn add(&mut self, record: ConnectionRecord) {
        self.connections.insert(record.id.clone(), record);
    }

    pub fn list(&self) -> Vec<ConnectionRecord> {
        self.connections.values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<&ConnectionRecord> {
        self.connections.get(id)
    }

    pub fn remove(&mut self, id: &str) -> Option<ConnectionRecord> {
        self.connections.remove(id)
    }
}
