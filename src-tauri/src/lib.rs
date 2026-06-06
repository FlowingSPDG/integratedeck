mod commands;
mod orchestrator;
mod paths;
mod state;

use std::sync::Arc;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

pub use orchestrator::Orchestrator;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("integratedeck=info".parse().unwrap()))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let orch = Orchestrator::new(&handle).await?;
                app.manage(Arc::new(tokio::sync::RwLock::new(orch)));
                Ok::<(), anyhow::Error>(())
            })
            .map_err(|e| e.to_string())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::list_surfaces,
            commands::get_profile,
            commands::save_profile,
            commands::scan_plugins,
            commands::load_sd_plugin,
            commands::bind_slot_sd,
            commands::trigger_slot,
            commands::list_connections,
            commands::add_connection,
            commands::execute_companion_action,
            commands::sidecar_ping,
            commands::get_pi_url,
            commands::register_mock_surface,
            commands::create_slot,
            commands::export_profile,
            commands::import_profile,
            commands::apply_surface_input,
        ])
        .run(tauri::generate_context!())
        .expect("error while running integratedeck");
}
