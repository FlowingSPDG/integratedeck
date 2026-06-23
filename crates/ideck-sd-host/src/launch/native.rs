use std::path::Path;
use std::process::Stdio;

use tokio::process::{Child, Command};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub async fn spawn_native(
    exe: &Path,
    plugin_dir: &Path,
    port: u16,
    plugin_uuid: &str,
    register_event: &str,
    info: &str,
) -> std::io::Result<Child> {
    let mut cmd = Command::new(exe);
    cmd.arg("-port")
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
