use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionRecord {
    pub id: String,
    pub module_id: String,
    pub label: String,
    #[serde(default)]
    pub config: Value,
    pub enabled: bool,
}

#[derive(Default)]
pub struct ConnectionRegistry {
    records: HashMap<String, ConnectionRecord>,
}

impl ConnectionRegistry {
    pub fn add(&mut self, record: ConnectionRecord) {
        self.records.insert(record.id.clone(), record);
    }

    pub fn remove(&mut self, id: &str) -> Option<ConnectionRecord> {
        self.records.remove(id)
    }

    pub fn list(&self) -> Vec<ConnectionRecord> {
        let mut records: Vec<_> = self.records.values().cloned().collect();
        records.sort_by(|a, b| a.label.cmp(&b.label));
        records
    }

    pub fn get(&self, id: &str) -> Option<&ConnectionRecord> {
        self.records.get(id)
    }
}
