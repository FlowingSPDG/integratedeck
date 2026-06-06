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

pub fn sidecar_dir() -> PathBuf {
    // Development: repo sidecar/; production: resource dir
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sidecar");
    if dev.join("dist/index.js").exists() {
        return dev;
    }
    PathBuf::from("sidecar")
}

pub fn node_binary() -> PathBuf {
    which_node().unwrap_or_else(|| PathBuf::from("node"))
}

fn which_node() -> Option<PathBuf> {
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

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(profiles_dir())?;
    std::fs::create_dir_all(sd_plugins_dir())?;
    std::fs::create_dir_all(companion_modules_dir())?;
    Ok(())
}
