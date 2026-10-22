mod autostart;
mod commands;
mod conflict_check;
mod errors;
mod hub;
mod logging;
mod orchestrator;
mod paths;
mod plugin_host;
mod settings_store;
mod state;
mod windows;

use std::sync::Arc;

use tauri::{App, Manager};
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

pub use orchestrator::Orchestrator;

/// Log to stdout and to a rotating file so a packaged app leaves a trace.
fn init_tracing() {
    let filter = EnvFilter::from_default_env().add_directive("integratedeck=info".parse().unwrap());
    match logging::LogFileMaker::open(&paths::log_file()) {
        Ok(file) => {
            use tracing_subscriber::fmt::writer::MakeWriterExt;
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_writer(file.and(std::io::stdout))
                .init();
        }
        Err(error) => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::stdout)
                .init();
            tracing::warn!("log file unavailable: {error}");
        }
    }

    // Panics in commands or plugin tasks otherwise vanish in a packaged build;
    // record them before the default handler prints to stderr.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        default_hook(info);
    }));

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        exe = %std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "?".into()),
        "integratedeck starting"
    );
}

fn start_orchestrator(app: &App) -> Result<(), String> {
    let handle = app.handle().clone();
    let orch =
        tauri::async_runtime::block_on(Orchestrator::new(&handle)).map_err(|e| e.to_string())?;
    let state = Arc::new(RwLock::new(orch));
    Orchestrator::start_companion_event_drain(state.clone());
    Orchestrator::start_background_init(state.clone());
    Orchestrator::start_usb_watch(state.clone());
    Orchestrator::start_plugin_restart_worker(state.clone());
    app.manage(state);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    // Keep the login registration in sync with the saved preference (it may
    // have been removed externally, or the binary may have moved).
    let settings = settings_store::load_global_settings();
    if let Err(error) = autostart::apply(settings.launch_on_startup) {
        tracing::warn!("launch-at-login: {error}");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Prefer a Node runtime shipped with the app for Companion and Node
            // SDK plugins; an explicit IDECK_NODE always wins.
            if std::env::var("IDECK_NODE").is_err() {
                if let Ok(resource_dir) = app.path().resource_dir() {
                    let node = resource_dir.join("node").join(if cfg!(windows) {
                        "node.exe"
                    } else {
                        "node"
                    });
                    if node.is_file() {
                        std::env::set_var("IDECK_NODE", node);
                    }
                }
            }
            windows::ensure_main_window(app)?;
            plugin_host::ensure_plugin_host_window(app)?;
            app.manage(plugin_host::install(app.handle())?);
            // Start the orchestrator before building the hidden settings webview:
            // each WebView costs tens of seconds on a cold machine, and devices
            // should connect (and light up) while that happens.
            start_orchestrator(app)?;
            // Creating the settings webview blocks the main thread for ~15-20s,
            // so schedule it once setup has returned. Devices and plugin host
            // windows can then be created first.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                let for_main = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    if let Err(error) = windows::ensure_settings_window(&for_main) {
                        tracing::warn!("settings window not created: {error}");
                    }
                });
            });
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
            commands::open_logs_folder,
            commands::open_companion_modules_folder,
            commands::scan_hid_devices,
            commands::list_surface_drivers,
            commands::scan_surface_devices,
            commands::connect_hid_device,
            commands::connect_surface_device,
            commands::ensure_surface_slots,
            commands::set_encoder_led,
            commands::set_studio_lcd,
            commands::get_last_nfc,
            commands::get_last_surface_input,
            commands::clear_surface_input_history,
            commands::get_runtime_status,
            commands::unload_sd_plugin,
            commands::list_action_library,
            commands::list_companion_feedbacks,
            commands::disconnect_hid_device,
            commands::load_sd_plugin,
            commands::bind_slot_sd,
            commands::bind_slot_companion,
            commands::bind_slot_companion_preset,
            commands::update_companion_options,
            commands::set_slot_companion_feedback,
            commands::clear_slot_companion_feedback,
            commands::bind_encoder_companion,
            commands::bind_encoder_companion_preset,
            commands::bind_encoder_sd,
            commands::bind_encoder_builtin,
            commands::simulate_encoder,
            commands::simulate_encoder_rotate,
            commands::unbind_encoder,
            commands::bind_slot_builtin,
            commands::update_slot_appearance,
            commands::update_multi_action,
            commands::set_switch_page_target,
            commands::add_multi_action_step,
            commands::unbind_slot,
            commands::trigger_slot,
            commands::simulate_slot_key,
            commands::list_connections,
            commands::get_companion_definitions,
            commands::list_companion_status,
            commands::start_vsd_server,
            commands::stop_vsd_server,
            commands::get_vsd_status,
            commands::unregister_surface,
            commands::add_connection,
            commands::get_connection_config_fields,
            commands::update_connection_config,
            commands::remove_connection,
            commands::execute_companion_action,
            commands::companion_ping,
            commands::get_pi_url,
            commands::get_pi_context,
            commands::focus_pi_slot,
            commands::list_variables,
            commands::get_cell_visuals,
            commands::get_dial_visuals,
            commands::get_property_inspector_url,
            commands::list_plugin_actions,
            commands::update_slot_settings,
            commands::set_active_page,
            commands::create_page,
            commands::rename_page,
            commands::delete_page,
            commands::set_surface_page,
            commands::register_mock_surface,
            commands::create_slot,
            commands::delete_slot,
            commands::apply_slot_snapshot,
            commands::transfer_slot_cell,
            commands::export_profile,
            commands::import_profile,
            commands::apply_surface_input,
            commands::open_settings_window,
            plugin_host::focus_plugin_host_window,
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
        .build(tauri::generate_context!())
        .expect("error while building integratedeck")
        .run(|app_handle, event| {
            // Plugin child processes are not killed by the OS when the app
            // exits, so terminate them explicitly (otherwise every restart
            // leaves a plugin process behind).
            if matches!(event, tauri::RunEvent::Exit) {
                let state = app_handle
                    .try_state::<Arc<RwLock<Orchestrator>>>()
                    .map(|state| state.inner().clone());
                if let Some(state) = state {
                    tauri::async_runtime::block_on(async move {
                        let mut o = state.write().await;
                        o.unload_sd_plugin(None).await.ok();
                        o.shutdown_companion().await;
                    });
                }
            }
        });
}
