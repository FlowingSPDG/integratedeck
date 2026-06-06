use std::path::Path;

/// Scan a directory for companion-module-* folders.
pub fn scan_module_dirs(root: &Path) -> Vec<std::path::PathBuf> {
    let mut modules = Vec::new();
    if !root.is_dir() {
        return modules;
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("companion-module-") && entry.path().is_dir() {
                modules.push(entry.path());
            }
        }
    }
    modules
}

/// Scan for .sdPlugin bundles.
pub fn scan_sd_plugins(root: &Path) -> Vec<std::path::PathBuf> {
    let mut plugins = Vec::new();
    if !root.is_dir() {
        return plugins;
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.extension().map(|e| e == "sdPlugin").unwrap_or(false)
                || name.ends_with(".sdPlugin")
            {
                plugins.push(path);
            }
        }
    }
    plugins
}
