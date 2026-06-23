//! Multi-plugin supervisor for Stream Deck brokers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{BrokerEvent, StreamDeckBroker};

#[derive(Debug, Clone)]
pub struct LoadedSdPlugin {
    pub path: PathBuf,
    pub name: String,
    pub plugin_uuid: String,
    pub port: u16,
}

pub struct PluginSupervisor {
    plugins: HashMap<String, Arc<StreamDeckBroker>>,
    meta: HashMap<String, LoadedSdPlugin>,
    drain_handles: HashMap<String, JoinHandle<()>>,
}

impl PluginSupervisor {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            meta: HashMap::new(),
            drain_handles: HashMap::new(),
        }
    }

    pub fn get(&self, uuid: &str) -> Option<Arc<StreamDeckBroker>> {
        self.plugins.get(uuid).cloned()
    }

    pub fn primary(&self) -> Option<Arc<StreamDeckBroker>> {
        self.plugins.values().next().cloned()
    }

    pub fn primary_meta(&self) -> Option<&LoadedSdPlugin> {
        self.meta.values().next()
    }

    pub fn meta(&self, uuid: &str) -> Option<&LoadedSdPlugin> {
        self.meta.get(uuid)
    }

    pub fn list(&self) -> Vec<LoadedSdPlugin> {
        self.meta.values().cloned().collect()
    }

    pub fn insert(
        &mut self,
        meta: LoadedSdPlugin,
        broker: Arc<StreamDeckBroker>,
        drain: JoinHandle<()>,
    ) {
        let uuid = meta.plugin_uuid.clone();
        if let Some(old) = self.drain_handles.remove(&uuid) {
            old.abort();
        }
        self.drain_handles.insert(uuid.clone(), drain);
        self.plugins.insert(uuid.clone(), broker);
        self.meta.insert(uuid, meta);
    }

    pub async fn unload(&mut self, uuid: &str) {
        if let Some(h) = self.drain_handles.remove(uuid) {
            h.abort();
        }
        self.plugins.remove(uuid);
        self.meta.remove(uuid);
    }

    pub async fn unload_all(&mut self) {
        for (_, h) in self.drain_handles.drain() {
            h.abort();
        }
        self.plugins.clear();
        self.meta.clear();
    }

    pub fn start_drain<F, Fut>(
        events: mpsc::UnboundedReceiver<BrokerEvent>,
        mut handler: F,
    ) -> JoinHandle<()>
    where
        F: FnMut(BrokerEvent) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        tokio::spawn(async move {
            let mut rx = events;
            while let Some(ev) = rx.recv().await {
                handler(ev).await;
            }
        })
    }
}

impl Default for PluginSupervisor {
    fn default() -> Self {
        Self::new()
    }
}
