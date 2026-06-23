mod commands;
mod conflict_check;
mod errors;
mod hub;
mod orchestrator;
mod paths;
mod settings_store;
mod state;
mod windows;

use std::sync::Arc;

use tauri::{App, Manager};
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

pub use orchestrator::Orchestrator;

type OrchState = Arc<RwLock<Orchestrator>>;

fn start_orchestrator(app: &App) -> Result<(), String> {
    let handle = app.handle().clone();
    let orch =
        tauri::async_runtime::block_on(Orchestrator::new(&handle)).map_err(|e| e.to_string())?;
    let state = Arc::new(RwLock::new(orch));
    Orchestrator::start_companion_event_drain(state.clone());
    Orchestrator::start_background_init(state.clone());
    Orchestrator::start_usb_watch(state.clone());
    app.manage(state);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("integratedeck=info".parse().unwrap()))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            windows::ensure_main_window(app)?;
            windows::ensure_settings_window(app)?;
            start_orchestrator(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::check_startup_conflicts,
            commands::list_surfaces,
            commands::get_profile,
            commands::save_profile,
            commands::scan_plugins,
            commands::open_plugins_folder,
            commands::open_companion_modules_folder,
            commands::scan_hid_devices,
            commands::list_surface_drivers,
            commands::connect_hid_device,
            commands::get_runtime_status,
            commands::unload_sd_plugin,
            commands::list_action_library,
            commands::disconnect_hid_device,
            commands::load_sd_plugin,
            commands::bind_slot_sd,
            commands::bind_slot_companion,
            commands::bind_slot_builtin,
            commands::update_slot_appearance,
            commands::update_multi_action,
            commands::set_switch_page_target,
            commands::add_multi_action_step,
            commands::unbind_slot,
            commands::trigger_slot,
            commands::list_connections,
            commands::add_connection,
            commands::remove_connection,
            commands::execute_companion_action,
            commands::companion_ping,
            commands::get_pi_url,
            commands::get_pi_context,
            commands::focus_pi_slot,
            commands::list_variables,
            commands::get_cell_visuals,
            commands::get_property_inspector_url,
            commands::list_plugin_actions,
            commands::update_slot_settings,
            commands::set_active_page,
            commands::register_mock_surface,
            commands::create_slot,
            commands::delete_slot,
            commands::apply_slot_snapshot,
            commands::export_profile,
            commands::import_profile,
            commands::apply_surface_input,
            commands::open_settings_window,
            commands::get_global_settings,
            commands::set_global_settings,
            commands::list_settings_devices,
            commands::get_settings_device,
            commands::set_device_label,
            commands::set_device_brightness,
            commands::connect_settings_device,
            commands::export_device_preset,
            commands::import_device_preset,
            commands::load_saved_device_preset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running integratedeck");
}
