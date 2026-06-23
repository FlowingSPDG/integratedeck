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

pub fn cache_dir() -> PathBuf {
    data_dir().join("cache")
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(profiles_dir())?;
    std::fs::create_dir_all(sd_plugins_dir())?;
    std::fs::create_dir_all(companion_modules_dir())?;
    std::fs::create_dir_all(global_settings_dir())?;
    std::fs::create_dir_all(device_presets_dir())?;
    std::fs::create_dir_all(cache_dir())?;
    Ok(())
}
