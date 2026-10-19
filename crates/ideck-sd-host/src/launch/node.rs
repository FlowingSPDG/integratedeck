//! Node SDK plugin launch.
//!
//! Official plugins ship a bundled `.js` entry and declare `"Nodejs"` in the
//! manifest. They expect the same CLI arguments as the official app and open a
//! WebSocket back to the broker themselves.

use std::path::Path;
use std::process::Stdio;

use tokio::process::{Child, Command};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Resolve the Node executable: `IDECK_NODE`, then a bundled runtime in
/// `$RESOURCE/node`, then `node` from `PATH`.
pub fn node_binary() -> String {
    if let Ok(explicit) = std::env::var("IDECK_NODE") {
        if !explicit.trim().is_empty() {
            return explicit;
        }
    }
    if let Ok(resource) = std::env::var("RESOURCE") {
        let candidate = Path::new(&resource).join("node").join(if cfg!(windows) {
            "node.exe"
        } else {
            "node"
        });
        if candidate.is_file() {
            return candidate.to_string_lossy().to_string();
        }
    }
    "node".to_string()
}

pub async fn spawn_node(
    script: &Path,
    plugin_dir: &Path,
    port: u16,
    plugin_uuid: &str,
    register_event: &str,
    info: &str,
) -> std::io::Result<Child> {
    let mut cmd = Command::new(node_binary());
    cmd.arg(script)
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
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    cmd.spawn()
}
