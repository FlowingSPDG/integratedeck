use std::path::Path;
use std::process::Stdio;

use tokio::process::{Child, Command};

pub async fn spawn_native(
    exe: &Path,
    plugin_dir: &Path,
    port: u16,
    plugin_uuid: &str,
    register_event: &str,
    info: &str,
) -> std::io::Result<Child> {
    Command::new(exe)
        .arg("-port")
        .arg(port.to_string())
        .arg("-pluginUUID")
        .arg(plugin_uuid)
        .arg("-registerEvent")
        .arg(register_event)
        .arg("-info")
        .arg(info)
        .current_dir(plugin_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}
