use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("integratedeck")
}

pub fn profiles_dir() -> PathBuf {
    data_dir().join("profiles")
}

pub fn sd_plugins_dir() -> PathBuf {
    data_dir().join("plugins").join("streamdeck")
}

pub fn companion_modules_dir() -> PathBuf {
    data_dir().join("plugins").join("companion")
}

pub fn global_settings_dir() -> PathBuf {
    data_dir().join("global-settings")
}

pub fn global_settings_file() -> PathBuf {
    global_settings_dir().join("app.json")
}

pub fn device_registry_file() -> PathBuf {
    global_settings_dir().join("devices.json")
}

pub fn device_presets_dir() -> PathBuf {
    profiles_dir().join("presets")
}

pub fn device_preset_file(device_key: &str) -> PathBuf {
    let safe = device_key.replace(':', "_");
    device_presets_dir().join(format!("{safe}.json"))
}

pub fn connections_file() -> PathBuf {
    data_dir().join("connections.json")
}

/// Directories scanned for Stream Deck plugins (primary data dir + optional dev/elgato paths).
pub fn sd_plugin_scan_roots() -> Vec<PathBuf> {
    let mut roots = vec![sd_plugins_dir()];

    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins/streamdeck");
    if dev.is_dir() {
        roots.push(dev);
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        let elgato = home
            .join("Library")
            .join("Application Support")
            .join("com.elgato.StreamDeck")
            .join("Plugins");
        if elgato.is_dir() {
            roots.push(elgato);
        }
    }

    #[cfg(target_os = "windows")]
    if let Some(roaming) = dirs::data_dir() {
        let elgato = roaming.join("Elgato").join("StreamDeck").join("Plugins");
        if elgato.is_dir() {
            roots.push(elgato);
        }
    }

    roots
}

pub fn companion_host_script() -> PathBuf {
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../companion-host/dist/index.js");
    if dev.exists() {
        return dev;
    }
    PathBuf::from("companion-host/dist/index.js")
}

pub fn node_binary() -> PathBuf {
    which_node().unwrap_or_else(|| PathBuf::from("node"))
}

pub fn validate_node_version() -> Result<(), String> {
    let node = node_binary();
    let output = std::process::Command::new(&node)
        .arg("-v")
        .output()
        .map_err(|e| format!("Node.js not found: {e}"))?;
    if !output.status.success() {
        return Err("Node.js version check failed".into());
    }
    let version = String::from_utf8_lossy(&output.stdout);
    let version = version.trim().trim_start_matches('v');
    let major: u32 = version
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if major < 18 {
        return Err(format!(
            "Node.js 18+ required for Companion modules (found {version})"
        ));
    }
    Ok(())
}

fn which_node() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::process::Command::new("where")
            .arg("node")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .next()
                    .map(|s| s.trim().to_string())
            })
            .map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("which")
            .arg("node")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .to_string()
            })
            .map(PathBuf::from)
    }
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(profiles_dir())?;
    std::fs::create_dir_all(sd_plugins_dir())?;
    std::fs::create_dir_all(companion_modules_dir())?;
    std::fs::create_dir_all(global_settings_dir())?;
    std::fs::create_dir_all(device_presets_dir())?;
    Ok(())
}
