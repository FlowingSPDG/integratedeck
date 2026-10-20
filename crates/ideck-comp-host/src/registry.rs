use std::path::{Path, PathBuf};

/// Scan a directory for Companion module folders.
///
/// Companion's own convention prefixes folders with `companion-module-`, but
/// modules installed from npm keep their package name (`studiocoast-vmix`), so
/// a folder that ships a manifest and an entry point is accepted as well.
pub fn scan_module_dirs(root: &Path) -> Vec<PathBuf> {
    let mut modules = Vec::new();
    if !root.is_dir() {
        return modules;
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if name.starts_with("companion-module-") || looks_like_module(&path) {
                modules.push(path);
            }
        }
    }
    modules.sort();
    modules
}

/// A folder counts as a module when it has a `package.json` with an entry point
/// (or a plain `dist/index.js`, which is what most published modules ship).
fn looks_like_module(path: &Path) -> bool {
    let manifest = path.join("package.json");
    if let Ok(text) = std::fs::read_to_string(&manifest) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if value.get("main").is_some() || value.get("exports").is_some() {
                return true;
            }
        }
    }
    path.join("dist/index.js").is_file() || path.join("index.js").is_file()
}

#[cfg(test)]
mod tests {
    use super::scan_module_dirs;

    #[test]
    fn accepts_prefixed_and_package_named_modules() {
        let root = std::env::temp_dir().join(format!("ideck-modscan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("companion-module-legacy")).unwrap();
        std::fs::create_dir_all(root.join("studiocoast-vmix")).unwrap();
        std::fs::write(
            root.join("studiocoast-vmix/package.json"),
            r#"{"name":"studiocoast-vmix","main":"dist/index.js"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("dist-only")).unwrap();
        std::fs::create_dir_all(root.join("dist-only/dist")).unwrap();
        std::fs::write(root.join("dist-only/dist/index.js"), "export default 1").unwrap();
        std::fs::create_dir_all(root.join("unrelated")).unwrap();
        std::fs::write(root.join("unrelated/readme.txt"), "not a module").unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();

        let names: Vec<String> = scan_module_dirs(&root)
            .iter()
            .filter_map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .collect();

        assert_eq!(
            names,
            vec![
                "companion-module-legacy".to_string(),
                "dist-only".to_string(),
                "studiocoast-vmix".to_string()
            ]
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
