use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("integratedeck")
}

pub fn profiles_dir() -> PathBuf {
    data_dir().join("profiles")
}

/// File holding one profile (profiles are stored one file per id).
pub fn profile_file_for(profile_id: &str) -> PathBuf {
    profiles_dir().join(format!("{profile_id}.json"))
}

/// Legacy single-profile file, migrated into `profile_file_for` on startup.
pub fn legacy_profile_file() -> PathBuf {
    profiles_dir().join("default.json")
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

/// Device presets belong to a profile: each profile keeps its own snapshot of
/// what a device shows, so a fresh profile does not inherit the old pages.
pub fn device_presets_dir_for(profile_id: &str) -> PathBuf {
    device_presets_dir().join(profile_id)
}

pub fn device_preset_file(profile_id: &str, device_key: &str) -> PathBuf {
    let safe = device_key.replace(':', "_");
    device_presets_dir_for(profile_id).join(format!("{safe}.json"))
}

pub fn connections_file() -> PathBuf {
    data_dir().join("connections.json")
}

/// Write a file by writing a sibling temp file and renaming it into place.
///
/// The profile, connection list and device presets are the user's only copy of
/// their setup; a plain `fs::write` truncated by a crash loses all of it. A
/// rename is atomic on the same volume (std uses MoveFileEx with
/// MOVEFILE_REPLACE_EXISTING on Windows).
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "integratedeck".to_string());
    let tmp = path.with_file_name(format!(".{file_name}.tmp"));
    std::fs::write(&tmp, data)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&tmp);
            Err(error)
        }
    }
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

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn log_file() -> PathBuf {
    logs_dir().join("integratedeck.log")
}

/// Last image pushed to a Studio LCD, so it can be restored on reconnect.
pub fn studio_lcd_file(surface_id: &str) -> PathBuf {
    cache_dir().join("lcd").join(format!("{surface_id}.jpg"))
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(profiles_dir())?;
    std::fs::create_dir_all(sd_plugins_dir())?;
    std::fs::create_dir_all(companion_modules_dir())?;
    std::fs::create_dir_all(global_settings_dir())?;
    std::fs::create_dir_all(device_presets_dir())?;
    std::fs::create_dir_all(cache_dir())?;
    std::fs::create_dir_all(logs_dir())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_atomic;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ideck-paths-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_atomic_replaces_the_file_and_leaves_no_temp() {
        let dir = temp_dir("atomic");
        let path = dir.join("default.json");

        write_atomic(&path, b"{\"v\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"v\":1}");

        // Overwriting an existing file must work (rename replaces on Windows).
        write_atomic(&path, b"{\"v\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"v\":2}");

        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_cleans_up_when_the_rename_fails() {
        let dir = temp_dir("atomic-fail");
        // A directory in the target's place makes the rename fail.
        let target = dir.join("default.json");
        std::fs::create_dir_all(&target).unwrap();

        assert!(write_atomic(&target, b"data").is_err());
        assert!(target.is_dir(), "existing target must be untouched");
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
