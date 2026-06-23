use std::path::{Path, PathBuf};

/// Scan a directory for companion-module-* folders.
pub fn scan_module_dirs(root: &Path) -> Vec<PathBuf> {
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
    modules.sort();
    modules
}

