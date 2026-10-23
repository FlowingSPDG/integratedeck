use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Companion-style variable store (subset).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VariableStore {
    values: HashMap<String, String>,
}

impl VariableStore {
    pub fn set(&mut self, id: impl Into<String>, value: impl Into<String>) {
        self.values.insert(id.into(), value.into());
    }

    pub fn get(&self, id: &str) -> Option<&str> {
        self.values.get(id).map(String::as_str)
    }

    pub fn all(&self) -> &HashMap<String, String> {
        &self.values
    }
}
