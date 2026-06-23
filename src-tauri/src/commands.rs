use std::collections::HashMap;
use std::sync::Arc;

use ideck_comp_host::ConnectionRecord;
use ideck_core::{PageId, Profile, Slot, SlotId, SlotLocator, SurfaceId, VisualState};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tokio::sync::RwLock;

use crate::errors::{friendly_anyhow, friendly_error};
use crate::orchestrator::{
    ActionLibrary, ConnectHidResult, LoadPluginResult, Orchestrator,
    PiContextDto, PluginActionInfo, RuntimeStatus, ScanResult, SettingsDeviceEntry,
};
use crate::settings_store::AppGlobalSettings;
use crate::paths;
use crate::settings_store;
use crate::windows;

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
pub async fn unload_sd_plugin(
    state: State<'_, OrchState>,
    plugin_uuid: Option<String>,
) -> Result<(), String> {
    let mut o = state.write().await;
    o.unload_sd_plugin(plugin_uuid).await
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
    o.load_sd_plugin(path.into(), state_clone)
        .await
        .map_err(friendly_anyhow)
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindSlotCompanionArgs {
    pub slot_id: String,
    pub connection_id: String,
    pub action_id: String,
    #[serde(default)]
    pub options: serde_json::Value,
}

#[tauri::command]
pub async fn bind_slot_companion(
    state: State<'_, OrchState>,
    args: BindSlotCompanionArgs,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let mut o = state.write().await;
    o.bind_slot_companion(
        slot_id,
        args.connection_id,
        args.action_id,
        args.options,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unbind_slot(state: State<'_, OrchState>, slot_id: String) -> Result<(), String> {
    let slot_id = parse_slot_id(&slot_id)?;
    let mut o = state.write().await;
    o.unbind_slot(slot_id).await.map_err(|e| e.to_string())
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
    o.add_connection(record.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(record)
}

#[tauri::command]
pub async fn remove_connection(
    state: State<'_, OrchState>,
    connection_id: String,
) -> Result<(), String> {
    let mut o = state.write().await;
    o.remove_connection(connection_id)
        .await
        .map_err(|e| e.to_string())
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
    o.execute_companion_action(&args.connection_id, &args.action_id, args.options)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn companion_ping(state: State<'_, OrchState>) -> Result<Option<String>, String> {
    let o = state.read().await;
    Ok(o.companion_ping().await)
}

#[tauri::command]
pub async fn get_pi_url(
    state: State<'_, OrchState>,
    plugin_uuid: Option<String>,
) -> Result<Option<String>, String> {
    let o = state.read().await;
    let port = plugin_uuid
        .as_deref()
        .and_then(|uuid| o.pi_websocket_port(uuid))
        .or_else(|| o.any_pi_websocket_port());
    Ok(port.map(|p| format!("ws://127.0.0.1:{p}")))
}

#[tauri::command]
pub async fn get_pi_context(
    state: State<'_, OrchState>,
    slot_id: String,
) -> Result<Option<PiContextDto>, String> {
    let slot_id = parse_slot_id(&slot_id)?;
    let o = state.read().await;
    Ok(o.get_pi_context(slot_id))
}

#[tauri::command]
pub async fn focus_pi_slot(
    state: State<'_, OrchState>,
    slot_id: Option<String>,
) -> Result<(), String> {
    let slot_id = slot_id.map(|s| parse_slot_id(&s)).transpose()?;
    let mut o = state.write().await;
    o.focus_pi_slot(slot_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_variables(
    state: State<'_, OrchState>,
) -> Result<HashMap<String, String>, String> {
    let o = state.read().await;
    Ok(o.variables().clone())
}

#[tauri::command]
pub async fn get_cell_visuals(
    state: State<'_, OrchState>,
) -> Result<HashMap<String, VisualState>, String> {
    let o = state.read().await;
    Ok(o.get_cell_visuals().await)
}

#[tauri::command]
pub async fn list_action_library(state: State<'_, OrchState>) -> Result<ActionLibrary, String> {
    let o = state.read().await;
    o.list_action_library().await.map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyInspectorUrlArgs {
    pub plugin_uuid: String,
    pub action_uuid: String,
}

#[tauri::command]
pub async fn get_property_inspector_url(
    state: State<'_, OrchState>,
    args: PropertyInspectorUrlArgs,
) -> Result<Option<String>, String> {
    let o = state.read().await;
    Ok(o
        .property_inspector_path(&args.plugin_uuid, &args.action_uuid)
        .map(|p| p.display().to_string()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPluginActionsArgs {
    pub connection_id: Option<String>,
    pub plugin_uuid: Option<String>,
}

#[tauri::command]
pub async fn list_plugin_actions(
    state: State<'_, OrchState>,
    args: ListPluginActionsArgs,
) -> Result<Vec<PluginActionInfo>, String> {
    let o = state.read().await;
    o.list_plugin_actions(args.connection_id, args.plugin_uuid)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSlotSettingsArgs {
    pub slot_id: String,
    pub settings: serde_json::Value,
}

#[tauri::command]
pub async fn update_slot_settings(
    state: State<'_, OrchState>,
    args: UpdateSlotSettingsArgs,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let mut o = state.write().await;
    o.update_slot_settings(slot_id, args.settings)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_active_page(
    state: State<'_, OrchState>,
    page_id: String,
) -> Result<(), String> {
    let page_id = PageId(uuid::Uuid::parse_str(&page_id).map_err(|e| e.to_string())?);
    let mut o = state.write().await;
    o.set_active_page(page_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_hid_devices(
    state: State<'_, OrchState>,
) -> Result<Vec<ideck_surface::HidDeviceDescriptor>, String> {
    let o = state.read().await;
    o.scan_hid_devices()
}

#[tauri::command]
pub async fn list_surface_drivers(
    state: State<'_, OrchState>,
) -> Result<Vec<ideck_surface::SurfaceDriverManifest>, String> {
    let o = state.read().await;
    Ok(o.list_surface_drivers())
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
    let reader = surface.clone().spawn_input_loop();

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

#[tauri::command]
pub async fn delete_slot(state: State<'_, OrchState>, slot_id: String) -> Result<(), String> {
    let slot_id = parse_slot_id(&slot_id)?;
    let mut o = state.write().await;
    o.delete_slot(slot_id).await.map_err(|e| e.to_string())
}

fn parse_slot_id(s: &str) -> Result<SlotId, String> {
    Ok(SlotId(
        uuid::Uuid::parse_str(s).map_err(|e| e.to_string())?,
    ))
}

#[tauri::command]
pub fn open_settings_window(app: AppHandle) -> Result<(), String> {
    windows::show_settings_window(&app)
}

#[tauri::command]
pub async fn get_global_settings() -> Result<AppGlobalSettings, String> {
    Ok(settings_store::load_global_settings())
}

#[tauri::command]
pub async fn set_global_settings(settings: AppGlobalSettings) -> Result<(), String> {
    settings_store::save_global_settings(&settings)
}

#[tauri::command]
pub async fn list_settings_devices(
    state: State<'_, OrchState>,
) -> Result<Vec<SettingsDeviceEntry>, String> {
    let o = state.read().await;
    Ok(o.list_settings_devices().await)
}

#[tauri::command]
pub async fn get_settings_device(
    state: State<'_, OrchState>,
    device_key: String,
) -> Result<Option<SettingsDeviceEntry>, String> {
    let o = state.read().await;
    Ok(o.get_settings_device(&device_key).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDeviceLabelArgs {
    pub device_key: String,
    pub label: String,
}

#[tauri::command]
pub async fn set_device_label(
    state: State<'_, OrchState>,
    args: SetDeviceLabelArgs,
) -> Result<(), String> {
    let mut o = state.write().await;
    o.set_device_label(&args.device_key, args.label).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDeviceBrightnessArgs {
    pub device_key: String,
    pub brightness: u8,
}

#[tauri::command]
pub async fn set_device_brightness(
    state: State<'_, OrchState>,
    args: SetDeviceBrightnessArgs,
) -> Result<(), String> {
    let mut o = state.write().await;
    o.set_device_brightness(&args.device_key, args.brightness)
        .await
}

#[tauri::command]
pub async fn connect_settings_device(
    state: State<'_, OrchState>,
    device_key: String,
) -> Result<ConnectHidResult, String> {
    let state_clone = state.inner().clone();
    let (result, surface) = {
        let mut o = state.write().await;
        o.connect_settings_device(&device_key).await?
    };
    let surface_id = SurfaceId(uuid::Uuid::parse_str(&result.surface_id).map_err(|e| e.to_string())?);
    let mut input_rx = surface.subscribe_inputs();
    let reader = surface.clone().spawn_input_loop();

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
pub async fn export_device_preset(
    state: State<'_, OrchState>,
    device_key: String,
) -> Result<String, String> {
    let o = state.read().await;
    o.export_device_preset(&device_key)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDevicePresetArgs {
    pub device_key: String,
    pub json: String,
}

#[tauri::command]
pub async fn import_device_preset(
    state: State<'_, OrchState>,
    args: ImportDevicePresetArgs,
) -> Result<(), String> {
    let mut o = state.write().await;
    o.import_device_preset(&args.device_key, &args.json).await
}

#[tauri::command]
pub async fn load_saved_device_preset(
    state: State<'_, OrchState>,
    device_key: String,
) -> Result<Option<String>, String> {
    let o = state.read().await;
    o.load_saved_device_preset(&device_key)
}
