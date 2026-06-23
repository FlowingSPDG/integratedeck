use std::path::{Component, Path, PathBuf};

pub struct ResolveContext {
    pub modules_root: PathBuf,
    pub referrer_path: Option<PathBuf>,
}

pub fn normalize_specifier(spec: &str) -> String {
    match spec {
        "events" => "node:events".into(),
        "path" => "node:path".into(),
        "fs" => "node:fs".into(),
        "url" => "node:url".into(),
        "ws" => "ws".into(),
        s if s.starts_with("node:") => s.to_string(),
        s => s.to_string(),
    }
}

pub fn is_builtin_shim(spec: &str) -> bool {
    matches!(
        spec,
        "node:path" | "node:url" | "node:fs" | "node:events" | "ws"
    )
}

pub fn url_to_path(url: &str) -> PathBuf {
    let stripped = url.strip_prefix("file:///").unwrap_or(url);
    PathBuf::from(stripped.replace('/', std::path::MAIN_SEPARATOR_STR))
}

fn normalize_path(base: &Path, rel: &str) -> PathBuf {
    let parent = if base.is_file() {
        base.parent().unwrap_or(base)
    } else {
        base
    };
    let mut out = parent.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(part) => out.push(part),
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out = PathBuf::from(comp.as_os_str()),
        }
    }
    out
}

fn find_package_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_file() {
        path.parent()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    loop {
        if current.join("package.json").is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

pub fn resolve_specifier(specifier: &str, ctx: &ResolveContext) -> Result<PathBuf, String> {
    let normalized = normalize_specifier(specifier);
    if is_builtin_shim(&normalized) {
        return Err(format!("built-in shim specifier: {normalized}"));
    }

    if normalized.starts_with('.')
        || normalized.starts_with('/')
        || normalized.starts_with("file:")
    {
        let base_path = ctx
            .referrer_path
            .clone()
            .unwrap_or_else(|| ctx.modules_root.clone());
        let resolved = if normalized.starts_with("file:") {
            url_to_path(&normalized)
        } else {
            normalize_path(&base_path, &normalized)
        };
        return Ok(resolved);
    }

    let search_root = ctx
        .referrer_path
        .as_ref()
        .and_then(|p| find_package_root(p))
        .unwrap_or_else(|| ctx.modules_root.clone());
    resolve_package(&search_root, &normalized)
}

fn resolve_package(search_root: &Path, name: &str) -> Result<PathBuf, String> {
    let mut current = search_root.to_path_buf();
    loop {
        let candidate = current.join("node_modules").join(name);
        if candidate.is_dir() {
            return resolve_package_entry(&candidate);
        }
        if !current.pop() {
            break;
        }
    }
    Err(format!("Cannot find module '{name}'"))
}

fn resolve_package_entry(dir: &Path) -> Result<PathBuf, String> {
    let pkg_path = dir.join("package.json");
    if pkg_path.is_file() {
        if let Ok(text) = std::fs::read_to_string(&pkg_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(exports) = json.get("exports") {
                    if let Some(main) = resolve_exports(exports) {
                        let path = dir.join(main);
                        if path.is_file() {
                            return Ok(path);
                        }
                    }
                }
                if let Some(main) = json.get("main").and_then(|v| v.as_str()) {
                    let path = dir.join(main);
                    if path.is_file() {
                        return Ok(path);
                    }
                }
            }
        }
    }
    for fallback in ["dist/main.js", "dist/index.js", "index.js", "main.js"] {
        let path = dir.join(fallback);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(format!("No entry for package {}", dir.display()))
}

fn resolve_exports(exports: &serde_json::Value) -> Option<String> {
    match exports {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(map) => {
            if let Some(import) = map.get("import").and_then(|v| v.as_str()) {
                return Some(import.to_string());
            }
            if let Some(require) = map.get("require").and_then(|v| v.as_str()) {
                return Some(require.to_string());
            }
            map.get(".")
                .and_then(|v| {
                    if let Some(s) = v.as_str() {
                        Some(s.to_string())
                    } else {
                        v.get("import")
                            .or_else(|| v.get("require"))
                            .and_then(|x| x.as_str())
                            .map(str::to_string)
                    }
                })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_relative_import() {
        let base = PathBuf::from("C:/mods/foo/dist/index.js");
        let out = normalize_path(&base, "../lib/bar.js");
        assert!(out.to_string_lossy().contains("lib"));
    }

    #[test]
    fn maps_bare_events_to_node_shim() {
        assert_eq!(normalize_specifier("events"), "node:events");
    }
}
