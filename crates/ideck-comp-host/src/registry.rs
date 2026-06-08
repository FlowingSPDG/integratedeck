use std::collections::HashSet;
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

fn is_sd_plugin_bundle(path: &Path, name: &str) -> bool {
    (path.extension().is_some_and(|e| e == "sdPlugin") || name.ends_with(".sdPlugin"))
        && path.join("manifest.json").is_file()
}

/// Scan for .sdPlugin bundles under a single directory (non-recursive).
pub fn scan_sd_plugins(root: &Path) -> Vec<PathBuf> {
    let mut plugins = Vec::new();
    if !root.is_dir() {
        return plugins;
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_sd_plugin_bundle(&path, &name) {
                plugins.push(path);
            }
        }
    }
    plugins.sort();
    plugins
}

/// Scan multiple roots and deduplicate by canonical path.
pub fn scan_sd_plugins_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut plugins = Vec::new();
    for root in roots {
        for path in scan_sd_plugins(root) {
            let key = std::fs::canonicalize(&path).unwrap_or(path.clone());
            if seen.insert(key) {
                plugins.push(path);
            }
        }
    }
    plugins.sort();
    plugins
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_scan_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ideck-scan-test-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_sd_plugins_finds_bundle_with_manifest() {
        let tmp = temp_scan_dir();
        let plugin = tmp.join("com.example.test.sdPlugin");
        fs::create_dir(&plugin).unwrap();
        fs::write(
            plugin.join("manifest.json"),
            r#"{"Name":"T","UUID":"u","Version":"1"}"#,
        )
        .unwrap();

        let found = scan_sd_plugins(&tmp);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], plugin);
        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn scan_sd_plugins_ignores_name_without_manifest() {
        let tmp = temp_scan_dir();
        let plugin = tmp.join("fake.sdPlugin");
        fs::create_dir(&plugin).unwrap();

        let found = scan_sd_plugins(&tmp);
        assert!(found.is_empty());
        let _ = fs::remove_dir_all(tmp);
    }
}
