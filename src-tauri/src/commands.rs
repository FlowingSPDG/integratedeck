use std::collections::HashMap;
use std::sync::Arc;

use ideck_comp_host::ConnectionRecord;
use ideck_core::{PageId, Profile, Slot, SlotId, SlotLocator, SurfaceId, VisualState};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::RwLock;

use ideck_surface::StreamDeckHidSurface;

use crate::errors::{friendly_anyhow, friendly_error};
use crate::orchestrator::{
    ConnectHidResult, LoadPluginResult, Orchestrator, RuntimeStatus, ScanResult,
};
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
    o.scan_plugins().map_err(friendly_error)
}

#[tauri::command]
pub async fn open_plugins_folder() -> Result<String, String> {
    paths::ensure_dirs().map_err(|e| e.to_string())?;
    let dir = paths::sd_plugins_dir();
    open_path_in_file_manager(&dir).map_err(|e| e.to_string())?;
    Ok(dir.display().to_string())
}

fn open_path_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(path).spawn()?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_runtime_status(state: State<'_, OrchState>) -> Result<RuntimeStatus, String> {
    let o = state.read().await;
    o.runtime_status().await.map_err(friendly_error)
}

#[tauri::command]
pub async fn unload_sd_plugin(state: State<'_, OrchState>) -> Result<(), String> {
    let mut o = state.write().await;
    o.unload_sd_plugin().await
}

#[tauri::command]
pub async fn disconnect_hid_device(
    state: State<'_, OrchState>,
    surface_id: String,
) -> Result<(), String> {
    let surface_id = SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let mut o = state.write().await;
    o.disconnect_hid_device(surface_id).await
}

#[tauri::command]
pub async fn load_sd_plugin(
    state: State<'_, OrchState>,
    path: String,
) -> Result<LoadPluginResult, String> {
    let state_clone = state.inner().clone();
    let mut o = state.write().await;
    let (result, events) = o
        .load_sd_plugin(path.into())
        .await
        .map_err(friendly_anyhow)?;

    let handle = tokio::spawn(async move {
        let mut rx = events;
        while let Some(ev) = rx.recv().await {
            let mut o = state_clone.write().await;
            o.apply_broker_event(ev).await.ok();
        }
    });
    o.set_broker_drain_handle(handle);
    Ok(result)
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
pub async fn get_cell_visuals(
    state: State<'_, OrchState>,
) -> Result<HashMap<String, VisualState>, String> {
    let o = state.read().await;
    Ok(o.get_cell_visuals().await)
}

#[tauri::command]
pub async fn get_property_inspector_url(
    state: State<'_, OrchState>,
    action_uuid: String,
) -> Result<Option<String>, String> {
    let o = state.read().await;
    Ok(o
        .property_inspector_path(&action_uuid)
        .map(|p| p.display().to_string()))
}

#[tauri::command]
pub async fn scan_hid_devices(
    state: State<'_, OrchState>,
) -> Result<Vec<ideck_surface::HidDeviceDescriptor>, String> {
    let o = state.read().await;
    o.scan_hid_devices()
}

#[tauri::command]
pub async fn connect_hid_device(
    state: State<'_, OrchState>,
    serial: String,
    kind: String,
) -> Result<ConnectHidResult, String> {
    let state_clone = state.inner().clone();
    let (result, surface) = {
        let mut o = state.write().await;
        o.connect_hid_device(serial, kind).await?
    };
    let surface_id = SurfaceId(uuid::Uuid::parse_str(&result.surface_id).map_err(|e| e.to_string())?);
    let mut input_rx = surface.subscribe_inputs();
    let reader_surface = surface.clone();

    let reader = tokio::spawn(async move {
        StreamDeckHidSurface::run_input_loop(reader_surface, 30.0).await;
    });
    let forward = tokio::spawn(async move {
        while let Ok(input) = input_rx.recv().await {
            let mut o = state_clone.write().await;
            o.apply_surface_input(surface_id, input).await.ok();
        }
    });

    {
        let mut o = state.write().await;
        o.store_hid_tasks(surface_id, vec![reader, forward]);
    }

    Ok(result)
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
