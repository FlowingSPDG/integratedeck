use std::sync::Arc;

use ideck_comp_host::ConnectionRecord;
use ideck_core::{PageId, Profile, Slot, SlotId, SlotLocator, SurfaceId};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::RwLock;

use crate::orchestrator::{LoadPluginResult, Orchestrator, ScanResult};
use crate::paths;

type OrchState = Arc<RwLock<Orchestrator>>;

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
}

#[tauri::command]
pub async fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: paths::data_dir().display().to_string(),
    }
}

#[tauri::command]
pub async fn list_surfaces(state: State<'_, OrchState>) -> Result<Vec<ideck_surface::SurfaceDescriptor>, String> {
    let o = state.read().await;
    Ok(o.list_surface_descriptors().await)
}

#[tauri::command]
pub async fn get_profile(state: State<'_, OrchState>) -> Result<Profile, String> {
    let o = state.read().await;
    Ok(o.profile().clone())
}

#[tauri::command]
pub async fn save_profile(state: State<'_, OrchState>) -> Result<(), String> {
    let o = state.read().await;
    o.save_profile().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_plugins(state: State<'_, OrchState>) -> Result<ScanResult, String> {
    let o = state.read().await;
    Ok(o.scan_plugins())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSdPluginArgs {
    pub path: String,
}

#[tauri::command]
pub async fn load_sd_plugin(
    state: State<'_, OrchState>,
    args: LoadSdPluginArgs,
) -> Result<LoadPluginResult, String> {
    let mut o = state.write().await;
    o.load_sd_plugin(args.path.into())
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindSlotSdArgs {
    pub slot_id: String,
    pub plugin_uuid: String,
    pub action_uuid: String,
}

#[tauri::command]
pub async fn bind_slot_sd(
    state: State<'_, OrchState>,
    args: BindSlotSdArgs,
) -> Result<String, String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let mut o = state.write().await;
    o.bind_slot_streamdeck(slot_id, args.plugin_uuid, args.action_uuid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn trigger_slot(state: State<'_, OrchState>, slot_id: String) -> Result<(), String> {
    let slot_id = parse_slot_id(&slot_id)?;
    let mut o = state.write().await;
    o.trigger_slot(slot_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_connections(state: State<'_, OrchState>) -> Result<Vec<ConnectionRecord>, String> {
    let o = state.read().await;
    Ok(o.connections())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddConnectionArgs {
    pub module_id: String,
    pub label: String,
    pub config: serde_json::Value,
}

#[tauri::command]
pub async fn add_connection(
    state: State<'_, OrchState>,
    args: AddConnectionArgs,
) -> Result<ConnectionRecord, String> {
    let record = ConnectionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        module_id: args.module_id,
        label: args.label,
        config: args.config,
        enabled: true,
    };
    let mut o = state.write().await;
    o.add_connection(record.clone());
    Ok(record)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteCompanionArgs {
    pub connection_id: String,
    pub action_id: String,
    pub options: serde_json::Value,
}

#[tauri::command]
pub async fn execute_companion_action(
    state: State<'_, OrchState>,
    args: ExecuteCompanionArgs,
) -> Result<(), String> {
    let o = state.read().await;
    o.execute_companion_via_sidecar(&args.connection_id, &args.action_id, args.options);
    Ok(())
}

#[tauri::command]
pub async fn sidecar_ping(state: State<'_, OrchState>) -> Result<Option<String>, String> {
    let o = state.read().await;
    Ok(o.sidecar_ping())
}

#[tauri::command]
pub async fn get_pi_url(state: State<'_, OrchState>) -> Result<Option<String>, String> {
    let o = state.read().await;
    Ok(o
        .pi_websocket_port()
        .map(|p| format!("ws://127.0.0.1:{p}")))
}

#[tauri::command]
pub async fn register_mock_surface(
    state: State<'_, OrchState>,
    name: String,
) -> Result<String, String> {
    let mut o = state.write().await;
    let id = o.register_mock_surface(name).await;
    Ok(id.0.to_string())
}

#[tauri::command]
pub async fn export_profile(state: State<'_, OrchState>) -> Result<String, String> {
    let o = state.read().await;
    serde_json::to_string_pretty(o.profile()).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
pub struct ImportProfileArgs {
    pub json: String,
}

#[tauri::command]
pub async fn import_profile(
    state: State<'_, OrchState>,
    args: ImportProfileArgs,
) -> Result<(), String> {
    let profile: Profile =
        serde_json::from_str(&args.json).map_err(|e| e.to_string())?;
    let mut o = state.write().await;
    *o.profile_mut() = profile;
    o.save_profile().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_surface_input(
    state: State<'_, OrchState>,
    surface_id: String,
    input_json: String,
) -> Result<(), String> {
    let surface_id = SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let input: ideck_surface::SurfaceInput =
        serde_json::from_str(&input_json).map_err(|e| e.to_string())?;
    let mut o = state.write().await;
    o.apply_surface_input(surface_id, input)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSlotArgs {
    pub surface_id: String,
    pub page_id: String,
    pub row: u32,
    pub column: u32,
}

#[tauri::command]
pub async fn create_slot(
    state: State<'_, OrchState>,
    args: CreateSlotArgs,
) -> Result<Slot, String> {
    let surface_id = SurfaceId(uuid::Uuid::parse_str(&args.surface_id).map_err(|e| e.to_string())?);
    let page_id = PageId(uuid::Uuid::parse_str(&args.page_id).map_err(|e| e.to_string())?);
    let mut o = state.write().await;
    let locator = SlotLocator {
        surface_id,
        page_id,
        row: args.row,
        column: args.column,
    };
    let slot = Slot::new(locator);
    if let Some(page) = o.profile_mut().pages.iter_mut().find(|p| p.id == page_id) {
        page.slots.insert(slot.id, slot.clone());
    }
    Ok(slot)
}

fn parse_slot_id(s: &str) -> Result<SlotId, String> {
    Ok(SlotId(
        uuid::Uuid::parse_str(s).map_err(|e| e.to_string())?,
    ))
}
