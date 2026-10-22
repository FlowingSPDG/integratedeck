use std::collections::HashMap;
use std::sync::Arc;

use ideck_comp_host::ConnectionRecord;
use ideck_core::{PageId, Profile, Slot, SlotId, SlotLocator, SurfaceId, VisualState};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tokio::sync::RwLock;

use crate::conflict_check;
use crate::errors::{friendly_anyhow, friendly_error};
use crate::orchestrator::{
    ActionLibrary, ConnectHidResult, LoadPluginResult, Orchestrator, PiContextDto,
    PluginActionInfo, RuntimeStatus, ScanResult, SettingsDeviceEntry,
};
use crate::paths;
use crate::settings_store;
use crate::settings_store::AppGlobalSettings;
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
pub async fn check_startup_conflicts() -> conflict_check::StartupConflictReport {
    tauri::async_runtime::spawn_blocking(conflict_check::detect_conflicting_apps)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("startup conflict check failed: {e}");
            conflict_check::StartupConflictReport {
                conflicts: Vec::new(),
            }
        })
}

#[tauri::command]
pub async fn list_surfaces(
    state: State<'_, OrchState>,
) -> Result<Vec<ideck_surface::SurfaceDescriptor>, String> {
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

#[tauri::command]
pub async fn open_logs_folder() -> Result<String, String> {
    paths::ensure_dirs().map_err(|e| e.to_string())?;
    let dir = paths::logs_dir();
    open_path_in_file_manager(&dir).map_err(|e| e.to_string())?;
    Ok(dir.display().to_string())
}

#[tauri::command]
pub async fn open_companion_modules_folder() -> Result<String, String> {
    paths::ensure_dirs().map_err(|e| e.to_string())?;
    let dir = paths::companion_modules_dir();
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
    // A manual load resets the crash-restart budget.
    o.reset_plugin_restart_attempts_for_path(&path);
    let result = o
        .load_sd_plugin(path.into(), state_clone)
        .await
        .map_err(friendly_anyhow)?;
    // Slots and dials that were bound to this plugin lost their contexts when it
    // stopped; rebind them once the plugin registers.
    o.request_plugin_rebind(&result.plugin_uuid);
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
    o.bind_slot_companion(slot_id, args.connection_id, args.action_id, args.options)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindSlotCompanionPresetArgs {
    pub slot_id: String,
    pub connection_id: String,
    pub preset_id: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[tauri::command]
pub async fn bind_slot_companion_preset(
    state: State<'_, OrchState>,
    args: BindSlotCompanionPresetArgs,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let mut o = state.write().await;
    o.bind_slot_companion_preset(slot_id, args.connection_id, args.preset_id, args.label)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindSlotBuiltinArgs {
    pub slot_id: String,
    pub action_id: String,
}

#[tauri::command]
pub async fn bind_slot_builtin(
    state: State<'_, OrchState>,
    args: BindSlotBuiltinArgs,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let mut o = state.write().await;
    o.bind_slot_builtin(slot_id, args.action_id)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSlotAppearanceArgs {
    pub slot_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub default_image_base64: Option<String>,
    #[serde(default)]
    pub clear_image: bool,
}

#[tauri::command]
pub async fn update_slot_appearance(
    state: State<'_, OrchState>,
    args: UpdateSlotAppearanceArgs,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let mut o = state.write().await;
    o.update_slot_appearance(
        slot_id,
        args.title,
        args.default_image_base64,
        args.clear_image,
    )
    .await
    .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMultiActionArgs {
    pub slot_id: String,
    pub steps: Vec<ideck_core::MultiActionStep>,
    #[serde(default)]
    pub delay_ms: Option<u32>,
}

#[tauri::command]
pub async fn update_multi_action(
    state: State<'_, OrchState>,
    args: UpdateMultiActionArgs,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let mut o = state.write().await;
    o.update_multi_action(slot_id, args.steps, args.delay_ms)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSwitchPageTargetArgs {
    pub slot_id: String,
    pub target_page_id: String,
}

#[tauri::command]
pub async fn set_switch_page_target(
    state: State<'_, OrchState>,
    args: SetSwitchPageTargetArgs,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let target_page_id =
        PageId(uuid::Uuid::parse_str(&args.target_page_id).map_err(|e| e.to_string())?);
    let mut o = state.write().await;
    o.set_switch_page_target(slot_id, target_page_id)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMultiActionStepArgs {
    pub slot_id: String,
    pub source: String,
    pub plugin_id: String,
    pub action_id: String,
    #[serde(default)]
    pub delay_before_ms: u32,
}

#[tauri::command]
pub async fn add_multi_action_step(
    state: State<'_, OrchState>,
    args: AddMultiActionStepArgs,
) -> Result<(), String> {
    use ideck_core::{ActionInstanceId, BindingKind, MultiActionStep};

    let slot_id = parse_slot_id(&args.slot_id)?;
    // Companion actions need the module's declared defaults; an empty options
    // object makes actions like Transition Mix do nothing.
    let companion_options = if args.source == "companion" {
        state
            .read()
            .await
            .companion_action_defaults(&args.plugin_id, &args.action_id)
            .await
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    let step_binding = match args.source.as_str() {
        "streamdeck" => BindingKind::StreamDeck {
            plugin_uuid: args.plugin_id,
            action_uuid: args.action_id,
            instance_id: ActionInstanceId::new(),
            settings: serde_json::json!({}),
        },
        "companion" => BindingKind::Companion {
            connection_id: args.plugin_id,
            action_id: args.action_id,
            options: companion_options,
        },
        // `action_id` carries the preset id in this case.
        "companion_preset" => BindingKind::CompanionPreset {
            connection_id: args.plugin_id,
            preset_id: args.action_id,
            label: None,
            color: None,
        },
        "builtin" => BindingKind::BuiltIn {
            action_id: args.action_id,
            settings: serde_json::json!({}),
        },
        _ => return Err(format!("unknown step source: {}", args.source)),
    };

    let mut o = state.write().await;
    let page_id = o
        .page_id_for_slot(slot_id)
        .ok_or_else(|| "slot not found".to_string())?;
    let surface_id = {
        let page = o
            .profile()
            .pages
            .iter()
            .find(|p| p.id == page_id)
            .ok_or_else(|| "page not found".to_string())?;
        page.slots
            .get(&slot_id)
            .map(|s| s.locator.surface_id)
            .ok_or_else(|| "slot not found".to_string())?
    };

    let steps = {
        let profile = o.profile_mut();
        let page = profile
            .pages
            .iter_mut()
            .find(|p| p.id == page_id)
            .ok_or_else(|| "page not found".to_string())?;
        let slot = page
            .slots
            .get_mut(&slot_id)
            .ok_or_else(|| "slot not found".to_string())?;
        let Some(ideck_core::Binding {
            kind: ideck_core::BindingKind::MultiAction { steps, .. },
        }) = &mut slot.binding
        else {
            return Err("slot is not a multi-action".into());
        };
        steps.push(MultiActionStep {
            binding: step_binding,
            delay_before_ms: args.delay_before_ms,
        });
        steps.clone()
    };

    o.update_multi_action(slot_id, steps, None)
        .await
        .map_err(|e| e.to_string())?;
    let _ = surface_id;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindEncoderCompanionArgs {
    pub surface_id: String,
    pub index: u32,
    pub connection_id: String,
    pub action_id: String,
    #[serde(default)]
    pub options: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindEncoderCompanionPresetArgs {
    pub surface_id: String,
    pub index: u32,
    pub connection_id: String,
    pub preset_id: String,
}

#[tauri::command]
pub async fn bind_encoder_companion_preset(
    state: State<'_, OrchState>,
    args: BindEncoderCompanionPresetArgs,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&args.surface_id).map_err(|e| e.to_string())?);
    let mut orchestrator = state.write().await;
    orchestrator
        .bind_encoder_companion_preset(surface_id, args.index, args.connection_id, args.preset_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn bind_encoder_companion(
    state: State<'_, OrchState>,
    args: BindEncoderCompanionArgs,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&args.surface_id).map_err(|e| e.to_string())?);
    let mut orchestrator = state.write().await;
    orchestrator
        .bind_encoder_companion(
            surface_id,
            args.index,
            args.connection_id,
            args.action_id,
            args.options,
        )
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateEncoderArgs {
    pub surface_id: String,
    pub index: u32,
    pub pressed: bool,
}

#[tauri::command]
pub async fn simulate_encoder(
    state: State<'_, OrchState>,
    args: SimulateEncoderArgs,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&args.surface_id).map_err(|e| e.to_string())?);
    let index = args.index;
    let pressed = args.pressed;
    let state_clone = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        Orchestrator::dispatch_simulate_encoder(state_clone, surface_id, index, pressed).await;
    });
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateEncoderRotateArgs {
    pub surface_id: String,
    pub index: u32,
    pub ticks: i32,
    #[serde(default)]
    pub pressed: bool,
}

#[tauri::command]
pub async fn simulate_encoder_rotate(
    state: State<'_, OrchState>,
    args: SimulateEncoderRotateArgs,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&args.surface_id).map_err(|e| e.to_string())?);
    let index = args.index;
    let ticks = args.ticks;
    let pressed = args.pressed;
    let state_clone = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        Orchestrator::dispatch_simulate_encoder_rotate(
            state_clone,
            surface_id,
            index,
            ticks,
            pressed,
        )
        .await;
    });
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindEncoderBuiltinArgs {
    pub surface_id: String,
    pub index: u32,
    pub action_id: String,
    #[serde(default)]
    pub settings: serde_json::Value,
}

#[tauri::command]
pub async fn bind_encoder_builtin(
    state: State<'_, OrchState>,
    args: BindEncoderBuiltinArgs,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&args.surface_id).map_err(|e| e.to_string())?);
    let mut orchestrator = state.write().await;
    orchestrator
        .bind_encoder_builtin(surface_id, args.index, args.action_id, args.settings)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindEncoderSdArgs {
    pub surface_id: String,
    pub index: u32,
    pub plugin_uuid: String,
    pub action_uuid: String,
}

#[tauri::command]
pub async fn bind_encoder_sd(
    state: State<'_, OrchState>,
    args: BindEncoderSdArgs,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&args.surface_id).map_err(|e| e.to_string())?);
    let mut orchestrator = state.write().await;
    orchestrator
        .bind_encoder_sd(surface_id, args.index, args.plugin_uuid, args.action_uuid)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnbindEncoderArgs {
    pub surface_id: String,
    pub index: u32,
}

#[tauri::command]
pub async fn unbind_encoder(
    state: State<'_, OrchState>,
    args: UnbindEncoderArgs,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&args.surface_id).map_err(|e| e.to_string())?);
    let mut orchestrator = state.write().await;
    orchestrator
        .unbind_encoder(surface_id, args.index)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCompanionOptionsArgs {
    pub slot_id: String,
    pub options: serde_json::Value,
}

#[tauri::command]
pub async fn update_companion_options(
    state: State<'_, OrchState>,
    args: UpdateCompanionOptionsArgs,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let mut orchestrator = state.write().await;
    orchestrator
        .update_companion_options(slot_id, args.options)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCompanionFeedbackArgs {
    pub slot_id: String,
    pub feedback_id: String,
    #[serde(default)]
    pub options: serde_json::Value,
}

#[tauri::command]
pub async fn set_slot_companion_feedback(
    state: State<'_, OrchState>,
    args: SetCompanionFeedbackArgs,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let mut orchestrator = state.write().await;
    orchestrator
        .set_slot_companion_feedback(slot_id, args.feedback_id, args.options)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_slot_companion_feedback(
    state: State<'_, OrchState>,
    slot_id: String,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&slot_id)?;
    let mut orchestrator = state.write().await;
    orchestrator
        .clear_slot_companion_feedback(slot_id)
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
    let state_clone = state.inner().clone();
    let mut o = state.write().await;
    o.trigger_slot(slot_id, Some(state_clone))
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateSlotKeyArgs {
    pub slot_id: String,
    /// `"key_down"` or `"key_up"`
    pub phase: String,
}

#[tauri::command]
pub async fn simulate_slot_key(
    state: State<'_, OrchState>,
    args: SimulateSlotKeyArgs,
) -> Result<(), String> {
    use crate::orchestrator::SimulateKeyPhase;

    let slot_id = parse_slot_id(&args.slot_id)?;
    let phase = match args.phase.as_str() {
        "key_down" => SimulateKeyPhase::KeyDown,
        "key_up" => SimulateKeyPhase::KeyUp,
        other => return Err(format!("invalid phase: {other}")),
    };
    let state_clone = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = Orchestrator::dispatch_simulate_slot_key(state_clone, slot_id, phase).await
        {
            tracing::warn!("simulate_slot_key: {e}");
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn list_companion_status(
    state: State<'_, OrchState>,
) -> Result<Vec<serde_json::Value>, String> {
    let o = state.read().await;
    Ok(o.companion_statuses())
}

#[tauri::command]
pub async fn start_vsd_server(
    state: State<'_, OrchState>,
    rows: u32,
    columns: u32,
    port: u16,
) -> Result<String, String> {
    let mut o = state.write().await;
    let (addr, events) = o.start_vsd_server(rows, columns, port).await?;
    drop(o);
    let state_clone = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        Orchestrator::run_vsd_event_loop(state_clone, events).await;
    });
    Ok(addr)
}

#[tauri::command]
pub async fn stop_vsd_server(state: State<'_, OrchState>) -> Result<(), String> {
    let mut o = state.write().await;
    o.stop_vsd_server();
    Ok(())
}

#[tauri::command]
pub async fn get_vsd_status(state: State<'_, OrchState>) -> Result<Option<String>, String> {
    let o = state.read().await;
    Ok(o.vsd_status())
}

#[tauri::command]
pub async fn unregister_surface(
    state: State<'_, OrchState>,
    surface_id: String,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let mut orchestrator = state.write().await;
    orchestrator
        .unregister_surface(surface_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_companion_definitions(
    state: State<'_, OrchState>,
    connection_id: String,
) -> Result<serde_json::Value, String> {
    let o = state.read().await;
    o.companion_definitions(&connection_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_connections(
    state: State<'_, OrchState>,
) -> Result<Vec<ConnectionRecord>, String> {
    let o = state.read().await;
    Ok(o.connections())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddConnectionArgs {
    pub module_id: String,
    pub label: String,
    pub config: serde_json::Value,
    #[serde(default)]
    pub secrets: serde_json::Value,
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
        secrets: args.secrets,
        enabled: true,
    };
    let mut o = state.write().await;
    o.add_connection(record.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(record)
}

#[tauri::command]
pub async fn get_connection_config_fields(
    state: State<'_, OrchState>,
    connection_id: String,
) -> Result<serde_json::Value, String> {
    let o = state.read().await;
    o.connection_config_fields(&connection_id)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConnectionConfigArgs {
    pub connection_id: String,
    pub config: serde_json::Value,
    #[serde(default)]
    pub secrets: serde_json::Value,
}

#[tauri::command]
pub async fn update_connection_config(
    state: State<'_, OrchState>,
    args: UpdateConnectionConfigArgs,
) -> Result<(), String> {
    let mut o = state.write().await;
    o.update_connection_config(args.connection_id, args.config, args.secrets)
        .await
        .map_err(|e| e.to_string())
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
    surface_id: String,
) -> Result<HashMap<String, VisualState>, String> {
    let surface_id = SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let o = state.read().await;
    Ok(o.get_cell_visuals(surface_id).await)
}

#[tauri::command]
pub async fn get_dial_visuals(
    state: State<'_, OrchState>,
    surface_id: String,
) -> Result<HashMap<u32, VisualState>, String> {
    let surface_id = SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let o = state.read().await;
    Ok(o.get_dial_visuals(surface_id))
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
    Ok(
        o.property_inspector_path(&args.plugin_uuid, &args.action_uuid)
            .map(|p| p.display().to_string()),
    )
}

#[tauri::command]
pub async fn list_companion_feedbacks(
    state: State<'_, OrchState>,
    connection_id: String,
) -> Result<Vec<PluginActionInfo>, String> {
    let o = state.read().await;
    o.list_companion_feedbacks(&connection_id)
        .await
        .map_err(|e| e.to_string())
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
pub async fn set_active_page(state: State<'_, OrchState>, page_id: String) -> Result<(), String> {
    let page_id = PageId(uuid::Uuid::parse_str(&page_id).map_err(|e| e.to_string())?);
    let mut o = state.write().await;
    o.set_active_page(page_id).await.map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePageArgs {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub parent_page_id: Option<String>,
}

/// Create a page and return its id. The frontend switches to it afterwards.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePageArgs {
    pub page_id: String,
    pub name: String,
}

#[tauri::command]
pub async fn rename_page(state: State<'_, OrchState>, args: RenamePageArgs) -> Result<(), String> {
    let page_id = PageId(uuid::Uuid::parse_str(&args.page_id).map_err(|e| e.to_string())?);
    let mut o = state.write().await;
    o.rename_page(page_id, &args.name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_page(state: State<'_, OrchState>, page_id: String) -> Result<(), String> {
    let page_id = ideck_core::PageId(uuid::Uuid::parse_str(&page_id).map_err(|e| e.to_string())?);
    let mut orchestrator = state.write().await;
    orchestrator
        .delete_page(page_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_page(
    state: State<'_, OrchState>,
    args: CreatePageArgs,
) -> Result<String, String> {
    let parent = args
        .parent_page_id
        .map(|raw| {
            uuid::Uuid::parse_str(&raw)
                .map(PageId)
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    let mut o = state.write().await;
    let page_id = o
        .create_page(args.name, parent)
        .await
        .map_err(|e| e.to_string())?;
    Ok(page_id.0.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSurfacePageArgs {
    pub surface_id: String,
    pub page_id: String,
}

#[tauri::command]
pub async fn set_surface_page(
    state: State<'_, OrchState>,
    args: SetSurfacePageArgs,
) -> Result<(), String> {
    let surface_id = SurfaceId(uuid::Uuid::parse_str(&args.surface_id).map_err(|e| e.to_string())?);
    let page_id = PageId(uuid::Uuid::parse_str(&args.page_id).map_err(|e| e.to_string())?);
    let mut o = state.write().await;
    o.set_surface_page(surface_id, page_id)
        .await
        .map_err(|e| e.to_string())
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
pub async fn scan_surface_devices(
    state: State<'_, OrchState>,
) -> Result<Vec<ideck_surface::DiscoveredDevice>, String> {
    let o = state.read().await;
    o.scan_surface_devices()
}

#[tauri::command]
pub async fn connect_hid_device(
    state: State<'_, OrchState>,
    serial: String,
    kind: String,
) -> Result<ConnectHidResult, String> {
    Orchestrator::connect_hid_with_loops(state.inner().clone(), serial, kind).await
}

#[tauri::command]
pub async fn connect_surface_device(
    state: State<'_, OrchState>,
    driver_id: String,
    device_id: String,
) -> Result<ConnectHidResult, String> {
    Orchestrator::connect_driver_with_loops(state.inner().clone(), driver_id, device_id).await
}

#[tauri::command]
pub async fn ensure_surface_slots(
    state: State<'_, OrchState>,
    surface_id: String,
) -> Result<usize, String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let mut orchestrator = state.write().await;
    orchestrator.ensure_surface_slots(surface_id).await
}

#[tauri::command]
pub async fn set_encoder_led(
    state: State<'_, OrchState>,
    surface_id: String,
    index: u8,
    r: u8,
    g: u8,
    b: u8,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let orchestrator = state.read().await;
    orchestrator.set_encoder_led(surface_id, index, r, g, b)
}

#[tauri::command]
pub async fn get_last_surface_input(
    state: State<'_, OrchState>,
    surface_id: String,
) -> Result<Option<serde_json::Value>, String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let orchestrator = state.read().await;
    Ok(orchestrator.last_surface_input(surface_id))
}

#[tauri::command]
pub async fn clear_surface_input_history(
    state: State<'_, OrchState>,
    surface_id: String,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let mut orchestrator = state.write().await;
    orchestrator.clear_surface_input_history(surface_id);
    Ok(())
}

#[tauri::command]
pub async fn get_last_nfc(
    state: State<'_, OrchState>,
    surface_id: String,
) -> Result<Option<String>, String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let orchestrator = state.read().await;
    Ok(orchestrator.last_nfc(surface_id))
}

#[tauri::command]
pub async fn set_studio_lcd(
    state: State<'_, OrchState>,
    surface_id: String,
    data_url: String,
) -> Result<(), String> {
    let surface_id =
        ideck_core::SurfaceId(uuid::Uuid::parse_str(&surface_id).map_err(|e| e.to_string())?);
    let image = ideck_core::image_from_data_url(&data_url, 1424)
        .ok_or_else(|| "invalid LCD image data URL".to_string())?;
    let orchestrator = state.read().await;
    orchestrator.set_studio_lcd(surface_id, &image)
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
    let profile: Profile = serde_json::from_str(&args.json).map_err(|e| e.to_string())?;
    // Apply through the orchestrator so routing, dial contexts and visuals are
    // resynced to the imported pages.
    let state_clone = state.inner().clone();
    let mut o = state.write().await;
    o.apply_imported_profile(profile, state_clone)
        .await
        .map_err(|e| e.to_string())
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
    Orchestrator::apply_surface_input(state.inner().clone(), surface_id, input).await;
    Ok(())
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
    o.save_profile().await.map_err(|e| e.to_string())?;
    Ok(slot)
}

#[tauri::command]
pub async fn delete_slot(state: State<'_, OrchState>, slot_id: String) -> Result<(), String> {
    let slot_id = parse_slot_id(&slot_id)?;
    let mut o = state.write().await;
    o.delete_slot(slot_id).await.map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySlotSnapshotArgs {
    pub slot_id: String,
    #[serde(default)]
    pub binding: Option<serde_json::Value>,
    #[serde(default)]
    pub appearance: ideck_core::SlotAppearance,
}

#[tauri::command]
pub async fn apply_slot_snapshot(
    state: State<'_, OrchState>,
    args: ApplySlotSnapshotArgs,
) -> Result<(), String> {
    let slot_id = parse_slot_id(&args.slot_id)?;
    let state_clone = state.inner().clone();
    let mut o = state.write().await;
    o.apply_slot_snapshot(slot_id, args.binding, args.appearance, state_clone)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSlotCellArgs {
    pub surface_id: String,
    pub page_id: String,
    pub from_row: u32,
    pub from_col: u32,
    pub to_row: u32,
    pub to_col: u32,
}

#[tauri::command]
pub async fn transfer_slot_cell(
    state: State<'_, OrchState>,
    args: TransferSlotCellArgs,
) -> Result<(), String> {
    let surface_id = SurfaceId(uuid::Uuid::parse_str(&args.surface_id).map_err(|e| e.to_string())?);
    let page_id = PageId(uuid::Uuid::parse_str(&args.page_id).map_err(|e| e.to_string())?);
    let state_clone = state.inner().clone();
    let mut o = state.write().await;
    o.transfer_slot_cell(
        surface_id,
        page_id,
        args.from_row,
        args.from_col,
        args.to_row,
        args.to_col,
        state_clone,
    )
    .await
    .map_err(|e| e.to_string())
}

fn parse_slot_id(s: &str) -> Result<SlotId, String> {
    Ok(SlotId(uuid::Uuid::parse_str(s).map_err(|e| e.to_string())?))
}

#[tauri::command]
pub fn open_settings_window(app: AppHandle) -> Result<(), String> {
    windows::show_settings_window(&app)
}

#[tauri::command]
pub async fn get_global_settings() -> Result<AppGlobalSettings, String> {
    let mut settings = settings_store::load_global_settings();
    // Report what the OS actually has, not just what we stored.
    settings.launch_on_startup = crate::autostart::is_enabled();
    Ok(settings)
}

#[tauri::command]
pub async fn set_global_settings(settings: AppGlobalSettings) -> Result<(), String> {
    settings_store::save_global_settings(&settings)?;
    // The toggle only means something once the OS knows about it.
    crate::autostart::apply(settings.launch_on_startup)
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
    let (serial, kind) = {
        let o = state.read().await;
        let record = o.resolve_device_record(&device_key)?;
        (record.serial, record.kind)
    };
    Orchestrator::connect_hid_with_loops(state.inner().clone(), serial, kind).await
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
    o.import_device_preset(&args.device_key, &args.json, state.inner().clone())
        .await
}

#[tauri::command]
pub async fn load_saved_device_preset(
    state: State<'_, OrchState>,
    device_key: String,
) -> Result<Option<String>, String> {
    let o = state.read().await;
    o.load_saved_device_preset(&device_key)
}
