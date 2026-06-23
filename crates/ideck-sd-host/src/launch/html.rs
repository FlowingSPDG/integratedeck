//! HTML plugin host spawns a minimal Node script that loads the page in puppeteer-like
//! WebSocket registration. On Windows integratedeck uses Tauri webview via host callback.

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

#[derive(Debug)]
pub struct HtmlPluginHandle {
    pub child: tokio::process::Child,
}

/// Spawn node helper that opens WebSocket to registerPlugin after loading HTML.
pub async fn spawn_html_host(
    html: &Path,
    plugin_dir: &Path,
    port: u16,
    plugin_uuid: &str,
    info: &str,
    node_binary: &Path,
    host_script: &Path,
) -> std::io::Result<HtmlPluginHandle> {
    let child = Command::new(node_binary)
        .arg(host_script)
        .arg(html)
        .arg(port.to_string())
        .arg(plugin_uuid)
        .arg(info)
        .current_dir(plugin_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    Ok(HtmlPluginHandle { child })
}
