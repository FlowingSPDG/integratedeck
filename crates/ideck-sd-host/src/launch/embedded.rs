use std::path::Path;

use ideck_js_runtime::{JsEngine, JsEngineConfig};
use serde_json::json;

const BOOTSTRAP: &str = include_str!("../../assets/sd_plugin_bootstrap.js");

#[derive(Debug, Clone, Copy)]
pub enum SdEntryMode {
    Script,
    Html,
}

pub fn spawn_embedded_plugin(
    plugin_dir: &Path,
    entry: &Path,
    mode: SdEntryMode,
    port: u16,
    plugin_uuid: &str,
    register_event: &str,
    info: &str,
) -> anyhow::Result<JsEngine> {
    let entry_path = entry.to_string_lossy().to_string();
    let plugin_dir_str = plugin_dir.to_string_lossy().to_string();
    let mode_str = match mode {
        SdEntryMode::Script => "script",
        SdEntryMode::Html => "html",
    };

    JsEngine::spawn(JsEngineConfig {
        module_root: plugin_dir.to_path_buf(),
        bootstrap: BOOTSTRAP,
        thread_name: "ideck-sd-js",
        with_websocket: true,
        event_emitter: None,
        startup_call: Some((
            "__ideckSdStart".into(),
            json!({
                "mode": mode_str,
                "entryPath": entry_path,
                "pluginDir": plugin_dir_str,
                "port": port,
                "pluginUUID": plugin_uuid,
                "registerEvent": register_event,
                "info": info,
            }),
        )),
    })
}
