use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppGlobalSettings {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub launch_on_startup: bool,
}

fn default_language() -> String {
    "ja".into()
}

impl Default for AppGlobalSettings {
    fn default() -> Self {
        Self {
            language: default_language(),
            launch_on_startup: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRecord {
    pub device_key: String,
    pub serial: String,
    pub kind: String,
    pub product: String,
    pub label: String,
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    pub last_seen: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_id: Option<String>,
}

fn default_brightness() -> u8 {
    50
}

pub fn load_global_settings() -> AppGlobalSettings {
    let path = paths::global_settings_file();
    if !path.exists() {
        return AppGlobalSettings::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save_global_settings(settings: &AppGlobalSettings) -> Result<(), String> {
    paths::ensure_dirs().map_err(|e| e.to_string())?;
    let data = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(paths::global_settings_file(), data).map_err(|e| e.to_string())
}

pub fn load_device_registry() -> Vec<DeviceRecord> {
    let path = paths::device_registry_file();
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save_device_registry(records: &[DeviceRecord]) -> Result<(), String> {
    paths::ensure_dirs().map_err(|e| e.to_string())?;
    let data = serde_json::to_string_pretty(records).map_err(|e| e.to_string())?;
    std::fs::write(paths::device_registry_file(), data).map_err(|e| e.to_string())
}

pub fn upsert_device_record(record: DeviceRecord) -> Result<(), String> {
    let mut records = load_device_registry();
    if let Some(existing) = records.iter_mut().find(|r| r.device_key == record.device_key) {
        *existing = record;
    } else {
        records.push(record);
    }
    records.sort_by(|a, b| a.label.cmp(&b.label).then(a.device_key.cmp(&b.device_key)));
    save_device_registry(&records)
}

pub fn device_key(kind: &str, serial: &str) -> String {
    format!("{kind}:{serial}")
}

pub fn now_iso() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}
