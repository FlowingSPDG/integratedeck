use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

const ELGATO_ENCRYPTED_MAGIC: &[u8] = b"ELGATO";

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("manifest not found")]
    NotFound,
    #[error("manifest missing required field: {0}")]
    MissingField(&'static str),
    #[error("manifest encrypted (Elgato binary format)")]
    Encrypted,
}

/// Elgato Stream Deck plugin manifest (`manifest.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct StreamDeckManifest {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "UUID")]
    pub uuid: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(default, rename = "Author")]
    pub author: Option<String>,
    #[serde(default, rename = "CodePath")]
    pub code_path: String,
    #[serde(default, rename = "CodePathMac")]
    pub code_path_mac: Option<String>,
    #[serde(default, rename = "CodePathWin")]
    pub code_path_win: Option<String>,
    #[serde(default, rename = "Actions")]
    pub actions: Vec<ManifestAction>,
    #[serde(default, rename = "Nodejs")]
    pub nodejs: Option<NodejsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestAction {
    #[serde(default, rename = "UUID")]
    pub uuid: Option<String>,
    #[serde(default, rename = "Name")]
    pub name: Option<String>,
    #[serde(default, rename = "PropertyInspectorPath")]
    pub property_inspector: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodejsConfig {
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(default, rename = "Debug")]
    pub debug: Option<Value>,
}

/// Minimal manifest fields for UI listing.
#[derive(Debug, Clone)]
pub struct ManifestSummary {
    pub name: String,
    pub uuid: Option<String>,
    pub version: Option<String>,
}

impl StreamDeckManifest {
    pub fn load(plugin_dir: &Path) -> Result<Self, ManifestError> {
        let parsed = parse_plugin_manifest(plugin_dir)?;
        if parsed.uuid.is_empty() {
            return Err(ManifestError::MissingField("UUID"));
        }
        Ok(Self {
            name: parsed.name,
            uuid: parsed.uuid,
            version: parsed.version.unwrap_or_else(|| "0.0.0".into()),
            author: parsed.author,
            code_path: parsed.code_path,
            code_path_mac: parsed.code_path_mac,
            code_path_win: parsed.code_path_win,
            actions: parsed.actions,
            nodejs: parsed.nodejs,
        })
    }

    /// Best-effort read for plugin scanning when strict load is not required.
    pub fn read_summary(plugin_dir: &Path) -> Option<ManifestSummary> {
        parse_plugin_manifest(plugin_dir).ok().map(|parsed| ManifestSummary {
            name: parsed.name,
            uuid: Some(parsed.uuid).filter(|u| !u.is_empty()),
            version: parsed.version,
        })
    }

    /// Action entries suitable for the action library UI.
    pub fn list_actions(plugin_dir: &Path) -> Result<Vec<ManifestAction>, ManifestError> {
        Ok(parse_plugin_manifest(plugin_dir)?.actions)
    }

    pub fn code_path_for_platform(&self, plugin_dir: &Path) -> PathBuf {
        let rel = self.code_path_relative();
        plugin_dir.join(rel)
    }

    /// Resolve the plugin entry file when manifest `CodePath` is missing or encrypted.
    pub fn resolve_code_path(&self, plugin_dir: &Path) -> Option<PathBuf> {
        let primary = self.code_path_for_platform(plugin_dir);
        if primary.is_file() {
            return Some(primary);
        }

        if !self.code_path.is_empty() {
            let fallback = plugin_dir.join(&self.code_path);
            if fallback.is_file() {
                return Some(fallback);
            }
        }

        for rel in HTML_ENTRY_CANDIDATES {
            let candidate = plugin_dir.join(rel);
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        for rel in NODE_ENTRY_CANDIDATES {
            let candidate = plugin_dir.join(rel);
            if !candidate.is_file() {
                continue;
            }
            if let Some(html) = paired_html_for_js(&candidate) {
                if html.is_file() {
                    return Some(html);
                }
            }
            return Some(candidate);
        }

        discover_native_binary(plugin_dir)
    }

    fn code_path_relative(&self) -> &str {
        #[cfg(target_os = "macos")]
        let override_path = self.code_path_mac.as_deref();
        #[cfg(target_os = "windows")]
        let override_path = self.code_path_win.as_deref();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let override_path: Option<&str> = None;

        override_path
            .filter(|p| !p.is_empty())
            .or({
                if self.code_path.is_empty() {
                    None
                } else {
                    Some(self.code_path.as_str())
                }
            })
            .unwrap_or(self.code_path.as_str())
    }

    pub fn action_by_uuid(&self, action_uuid: &str) -> Option<&ManifestAction> {
        self.actions
            .iter()
            .find(|a| a.uuid.as_deref() == Some(action_uuid))
    }
}

/// Stable plugin id for UI grouping when manifest UUID is absent.
pub fn effective_plugin_uuid(plugin_dir: &Path, manifest_uuid: Option<&str>) -> String {
    manifest_uuid
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| infer_uuid_from_bundle(plugin_dir).unwrap_or_else(|| {
            plugin_dir
                .to_string_lossy()
                .replace('\\', "/")
        }))
}

#[derive(Debug, Clone)]
struct ParsedManifest {
    name: String,
    uuid: String,
    version: Option<String>,
    author: Option<String>,
    code_path: String,
    code_path_mac: Option<String>,
    code_path_win: Option<String>,
    actions: Vec<ManifestAction>,
    nodejs: Option<NodejsConfig>,
}

fn parse_plugin_manifest(plugin_dir: &Path) -> Result<ParsedManifest, ManifestError> {
    let path = plugin_dir.join("manifest.json");
    if !path.exists() {
        return Err(ManifestError::NotFound);
    }

    let bytes = std::fs::read(&path)?;
    if is_elgato_encrypted(&bytes) {
        return parse_encrypted_manifest(plugin_dir);
    }

    let value = parse_manifest_json(&bytes)?;
    manifest_from_value(plugin_dir, &value)
}

fn parse_encrypted_manifest(plugin_dir: &Path) -> Result<ParsedManifest, ManifestError> {
    let localization = read_localization(plugin_dir);
    let uuid = infer_uuid_from_bundle(plugin_dir).ok_or(ManifestError::Encrypted)?;
    let name = localization
        .as_ref()
        .and_then(|loc| pick_string(loc, &["Name", "name"]))
        .or_else(|| humanize_bundle_name(plugin_dir))
        .ok_or(ManifestError::Encrypted)?;
    let actions = localization
        .as_ref()
        .map(actions_from_localization)
        .unwrap_or_default();
    Ok(ParsedManifest {
        name,
        uuid,
        version: None,
        author: None,
        code_path: String::new(),
        code_path_mac: None,
        code_path_win: None,
        actions,
        nodejs: None,
    })
}

fn manifest_from_value(plugin_dir: &Path, value: &Value) -> Result<ParsedManifest, ManifestError> {
    let localization = read_localization(plugin_dir);
    let uuid = plugin_uuid_from_value(value, plugin_dir);
    let name = plugin_name_from_value(value, plugin_dir, localization.as_ref());
    let mut actions = flatten_actions(value);
    if actions.is_empty() {
        if let Some(loc) = localization.as_ref() {
            actions = actions_from_localization(loc);
        }
    }

    Ok(ParsedManifest {
        name,
        uuid,
        version: pick_string(value, &["Version", "version"]),
        author: pick_string(value, &["Author", "author"]),
        code_path: pick_string(value, &["CodePath", "codePath"]).unwrap_or_default(),
        code_path_mac: pick_string(value, &["CodePathMac", "codePathMac"]),
        code_path_win: pick_string(value, &["CodePathWin", "codePathWin"]),
        actions,
        nodejs: value
            .get("Nodejs")
            .or_else(|| value.get("nodejs"))
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
    })
}

fn is_elgato_encrypted(bytes: &[u8]) -> bool {
    bytes.starts_with(ELGATO_ENCRYPTED_MAGIC)
}

fn parse_manifest_json(bytes: &[u8]) -> Result<Value, ManifestError> {
    if let Some(text) = decode_text(bytes) {
        if let Ok(value) = serde_json::from_str(&text) {
            return Ok(value);
        }
    }

    // Some manifests are UTF-16 without BOM.
    if bytes.len() >= 2 && bytes[0] == 0 {
        if let Ok(text) = String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>(),
        ) {
            return Ok(serde_json::from_str(text.trim_start_matches('\u{feff}'))?);
        }
    }

    Err(ManifestError::Json(serde_json::from_str::<Value>("").unwrap_err()))
}

fn decode_text(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(bytes[3..].to_vec()).ok();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16(&units).ok();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16(&units).ok();
    }
    String::from_utf8(bytes.to_vec()).ok()
}

fn read_localization(plugin_dir: &Path) -> Option<Value> {
    for lang in ["ja", "en"] {
        let path = plugin_dir.join(format!("{lang}.json"));
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Some(text) = decode_text(&bytes) else {
            continue;
        };
        if let Ok(value) = serde_json::from_str(&text) {
            return Some(value);
        }
    }
    None
}

fn plugin_uuid_from_value(value: &Value, plugin_dir: &Path) -> String {
    pick_string(
        value,
        &[
            "UUID",
            "Uuid",
            "uuid",
            "StreamdeckID",
            "StreamDeckID",
            "PluginUUID",
        ],
    )
    .unwrap_or_else(|| {
        infer_uuid_from_bundle(plugin_dir).unwrap_or_else(|| {
            plugin_dir
                .to_string_lossy()
                .replace('\\', "/")
        })
    })
}

fn plugin_name_from_value(
    value: &Value,
    plugin_dir: &Path,
    localization: Option<&Value>,
) -> String {
    pick_string(value, &["Name", "name"])
        .or_else(|| pick_string(value, &["Category", "category"]))
        .or_else(|| {
            localization.and_then(|loc| pick_string(loc, &["Name", "name"]))
        })
        .or_else(|| humanize_bundle_name(plugin_dir))
        .unwrap_or_else(|| "Unknown Plugin".into())
}

fn infer_uuid_from_bundle(plugin_dir: &Path) -> Option<String> {
    plugin_dir
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(".sdPlugin"))
        .map(str::to_string)
}

fn humanize_bundle_name(plugin_dir: &Path) -> Option<String> {
    infer_uuid_from_bundle(plugin_dir).map(|id| {
        let segment = id.rsplit('.').next().unwrap_or(&id);
        segment
            .split(['-', '_'])
            .filter(|part| !part.is_empty())
            .map(title_case_part)
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn title_case_part(part: &str) -> String {
    let mut chars = part.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
    }
}

fn flatten_actions(value: &Value) -> Vec<ManifestAction> {
    let mut actions = Vec::new();
    if let Some(items) = value
        .get("Actions")
        .or_else(|| value.get("actions"))
        .and_then(|v| v.as_array())
    {
        collect_actions(items, &mut actions);
    }
    actions
}

fn collect_actions(items: &[Value], out: &mut Vec<ManifestAction>) {
    for item in items {
        if let Some(nested) = item
            .get("Actions")
            .or_else(|| item.get("actions"))
            .and_then(|v| v.as_array())
        {
            collect_actions(nested, out);
            continue;
        }

        let uuid = pick_string(item, &["UUID", "Uuid", "uuid"]);
        let name = pick_string(item, &["Name", "name"]);
        let property_inspector =
            pick_string(item, &["PropertyInspectorPath", "propertyInspectorPath"]);

        if uuid.is_none() && name.is_none() {
            continue;
        }

        out.push(ManifestAction {
            uuid,
            name,
            property_inspector,
        });
    }
}

fn actions_from_localization(value: &Value) -> Vec<ManifestAction> {
    let Some(obj) = value.as_object() else {
        return Vec::new();
    };

    let mut actions: Vec<ManifestAction> = obj
        .iter()
        .filter(|(key, value)| looks_like_action_uuid(key) && value.is_object())
        .map(|(key, value)| ManifestAction {
            uuid: Some(key.clone()),
            name: pick_string(value, &["Name", "name"]),
            property_inspector: pick_string(value, &["PropertyInspectorPath", "propertyInspectorPath"]),
        })
        .collect();
    actions.sort_by(|a, b| {
        a.name
            .as_deref()
            .unwrap_or("")
            .cmp(b.name.as_deref().unwrap_or(""))
            .then_with(|| a.uuid.as_deref().unwrap_or("").cmp(b.uuid.as_deref().unwrap_or("")))
    });
    actions
}

fn looks_like_action_uuid(key: &str) -> bool {
    key.contains('.') && !matches!(key, "Name" | "Description")
}

const NODE_ENTRY_CANDIDATES: &[&str] = &[
    "bin/plugin.js",
    "bin/plugin.mjs",
    "source/main.js",
    "source/main.mjs",
    "plugin.js",
    "plugin.mjs",
    "index.js",
];

const HTML_ENTRY_CANDIDATES: &[&str] = &["source/main.html", "index.html", "app.html"];

fn paired_html_for_js(js_path: &Path) -> Option<PathBuf> {
    let stem = js_path.file_stem()?.to_string_lossy();
    js_path.parent().map(|dir| dir.join(format!("{stem}.html")))
}

fn discover_native_binary(plugin_dir: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut exes = collect_root_files_with_ext(plugin_dir, "exe")?;
        if exes.is_empty() {
            return None;
        }
        if exes.len() == 1 {
            return Some(exes.remove(0));
        }

        let bundle_stem = plugin_dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".sdPlugin"))
            .unwrap_or("")
            .to_ascii_lowercase();

        exes.sort_by(|a, b| {
            native_binary_priority(a, &bundle_stem)
                .cmp(&native_binary_priority(b, &bundle_stem))
                .reverse()
                .then_with(|| a.file_name().cmp(&b.file_name()))
        });
        exes.into_iter().next()
    }

    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::PermissionsExt;

        let entries = std::fs::read_dir(plugin_dir).ok()?;
        let mut candidates = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().is_some() {
                continue;
            }
            let metadata = std::fs::metadata(&path).ok()?;
            if metadata.permissions().mode() & 0o111 != 0 {
                candidates.push(path);
            }
        }
        if candidates.len() == 1 {
            return candidates.into_iter().next();
        }
        None
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
fn collect_root_files_with_ext(plugin_dir: &Path, ext: &str) -> Option<Vec<PathBuf>> {
    let entries = std::fs::read_dir(plugin_dir).ok()?;
    Some(
        entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
            })
            .collect(),
    )
}

#[cfg(target_os = "windows")]
fn native_binary_priority(path: &Path, bundle_stem: &str) -> u8 {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut score = 0u8;
    if name.starts_with("esd") {
        score = score.saturating_add(4);
    }
    if name.contains("bridge") {
        score = score.saturating_add(3);
    }
    if bundle_stem.contains(&name) || name.contains(bundle_stem) {
        score = score.saturating_add(2);
    }
    if name == bundle_stem {
        score = score.saturating_add(1);
    }
    score
}

fn pick_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        let Some(raw) = value.get(*key) else {
            continue;
        };
        let parsed = match raw {
            Value::String(s) => s.trim().to_string(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => continue,
        };
        if !parsed.is_empty() {
            return Some(parsed);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_elgato_uuid_field_names() {
        let json = r#"{
            "Name": "Test Plugin",
            "UUID": "com.example.plugin",
            "Version": "1.0.0",
            "CodePath": "bin/plugin.js",
            "Actions": [
                { "Name": "Action A", "UUID": "com.example.action" },
                { "Name": "Category Only" }
            ]
        }"#;
        let manifest: StreamDeckManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.uuid, "com.example.plugin");
        assert_eq!(manifest.name, "Test Plugin");
        assert_eq!(manifest.actions.len(), 2);
        assert_eq!(
            manifest.actions[0].uuid.as_deref(),
            Some("com.example.action")
        );
        assert!(manifest.actions[1].uuid.is_none());
    }

    #[test]
    fn parses_streamdeck_id_and_category_name() {
        let dir = std::env::temp_dir().join(format!(
            "ideck-manifest-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("manifest.json"),
            r#"{
                "Name": " ",
                "StreamdeckID": "com.example.tomato",
                "Version": "0.6.0",
                "CodePath": "index.html",
                "Category": "Tomato Timer",
                "Actions": [
                    { "Name": "Timer", "UUID": "com.example.tomato.clock" }
                ]
            }"#,
        )
        .unwrap();

        let summary = StreamDeckManifest::read_summary(&dir).expect("summary");
        assert_eq!(summary.name, "Tomato Timer");
        assert_eq!(summary.uuid.as_deref(), Some("com.example.tomato"));

        let manifest = StreamDeckManifest::load(&dir).expect("load");
        assert_eq!(manifest.uuid, "com.example.tomato");
        assert_eq!(manifest.actions.len(), 1);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn parses_localization_for_encrypted_manifest() {
        let dir = std::env::temp_dir().join(format!(
            "ideck-manifest-encrypted-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bundle = dir.join("com.example.chat.sdPlugin");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join("manifest.json"), b"ELGATO\x01\x00").unwrap();
        fs::write(
            bundle.join("en.json"),
            r#"{
                "Name": "Discord",
                "com.example.chat.mute": { "Name": "Mute" },
                "com.example.chat.deafen": { "Name": "Deafen" }
            }"#,
        )
        .unwrap();

        let summary = StreamDeckManifest::read_summary(&bundle).expect("summary");
        assert_eq!(summary.name, "Discord");
        assert_eq!(summary.uuid.as_deref(), Some("com.example.chat"));

        let actions = StreamDeckManifest::list_actions(&bundle).expect("actions");
        assert_eq!(actions.len(), 2);
        let ids: Vec<_> = actions.iter().filter_map(|a| a.uuid.as_deref()).collect();
        assert!(ids.contains(&"com.example.chat.mute"));
        assert!(ids.contains(&"com.example.chat.deafen"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resolves_code_path_from_common_node_layout() {
        let dir = std::env::temp_dir().join(format!(
            "ideck-codepath-node-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("manifest.json"), r#"{"Name":"P","UUID":"com.test","Version":"1"}"#).unwrap();
        fs::write(dir.join("bin/plugin.js"), "// plugin").unwrap();

        let manifest = StreamDeckManifest::load(&dir).expect("load");
        assert_eq!(
            manifest.resolve_code_path(&dir).unwrap(),
            dir.join("bin/plugin.js")
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resolves_code_path_for_encrypted_style_empty_manifest_path() {
        let dir = std::env::temp_dir().join(format!(
            "ideck-codepath-native-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bundle = dir.join("com.test.discord.sdPlugin");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join("manifest.json"), b"ELGATO\x01\x00").unwrap();
        fs::write(
            bundle.join("en.json"),
            r#"{"Name":"Discord","com.test.action":{"Name":"Mute"}}"#,
        )
        .unwrap();
        fs::write(bundle.join("ESDTest.exe"), "fake").unwrap();

        let manifest = StreamDeckManifest::load(&bundle).expect("load");
        assert_eq!(
            manifest.resolve_code_path(&bundle).unwrap(),
            bundle.join("ESDTest.exe")
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resolves_obs_style_source_main_js() {
        let dir = std::env::temp_dir().join(format!(
            "ideck-codepath-obs-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join("source")).unwrap();
        fs::write(
            dir.join("manifest.json"),
            r#"{"Name":"OBS","UUID":"com.elgato.obsstudio","Version":"1"}"#,
        )
        .unwrap();
        fs::write(dir.join("source/main.js"), "// main").unwrap();

        let manifest = StreamDeckManifest::load(&dir).expect("load");
        assert_eq!(
            manifest.resolve_code_path(&dir).unwrap(),
            dir.join("source/main.js")
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resolves_encrypted_obs_style_html_over_paired_js() {
        let dir = std::env::temp_dir().join(format!(
            "ideck-codepath-obs-encrypted-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bundle = dir.join("com.elgato.obsstudio.sdPlugin");
        fs::create_dir_all(bundle.join("source")).unwrap();
        fs::write(bundle.join("manifest.json"), b"ELGATO\x01\x00").unwrap();
        fs::write(
            bundle.join("en.json"),
            r#"{"Name":"OBS Studio","com.elgato.obsstudio.record":{"Name":"Record"}}"#,
        )
        .unwrap();
        fs::write(bundle.join("source/main.html"), "<html></html>").unwrap();
        fs::write(bundle.join("source/main.js"), "// main").unwrap();

        let manifest = StreamDeckManifest::load(&bundle).expect("load");
        assert_eq!(manifest.name, "OBS Studio");
        assert_eq!(
            manifest.resolve_code_path(&bundle).unwrap(),
            bundle.join("source/main.html")
        );

        let strategy =
            crate::launch::PluginLaunchStrategy::detect(&manifest, &bundle).expect("detect");
        match strategy {
            crate::launch::PluginLaunchStrategy::HtmlWebView { html } => {
                assert_eq!(html, bundle.join("source/main.html"));
            }
            other => panic!("expected html webview, got {other:?}"),
        }

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn launch_strategy_detects_discovered_native_binary() {
        let dir = std::env::temp_dir().join(format!(
            "ideck-launch-native-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bundle = dir.join("com.test.discord.sdPlugin");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join("manifest.json"), b"ELGATO\x01\x00").unwrap();
        fs::write(
            bundle.join("en.json"),
            r#"{"Name":"Discord","com.test.action":{"Name":"Mute"}}"#,
        )
        .unwrap();
        fs::write(bundle.join("ESDTest.exe"), "fake").unwrap();

        let manifest = StreamDeckManifest::load(&bundle).expect("load");
        let strategy =
            crate::launch::PluginLaunchStrategy::detect(&manifest, &bundle).expect("detect");
        match strategy {
            crate::launch::PluginLaunchStrategy::NativeBinary { exe } => {
                assert_eq!(exe, bundle.join("ESDTest.exe"));
            }
            other => panic!("expected native binary, got {other:?}"),
        }

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn effective_uuid_falls_back_to_bundle_name() {
        let dir = PathBuf::from("/tmp/com.vendor.plugin.sdPlugin");
        assert_eq!(
            effective_plugin_uuid(&dir, None),
            "com.vendor.plugin"
        );
    }

    #[test]
    fn parses_installed_elgato_plugins_when_available() {
        let root = std::env::var("APPDATA")
            .ok()
            .map(|appdata| PathBuf::from(appdata).join("Elgato/StreamDeck/Plugins"));
        let Some(root) = root.filter(|p| p.is_dir()) else {
            return;
        };

        for entry in std::fs::read_dir(&root).unwrap().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("sdPlugin") {
                continue;
            }
            let bundle = path.file_name().unwrap().to_string_lossy();
            let summary = StreamDeckManifest::read_summary(&path)
                .unwrap_or_else(|| panic!("summary failed for {bundle}"));
            assert!(
                !summary.name.trim().is_empty(),
                "empty plugin name for {bundle}"
            );
            assert!(
                summary.uuid.as_deref().is_some_and(|u| !u.is_empty()),
                "empty plugin uuid for {bundle}"
            );

            let actions = StreamDeckManifest::list_actions(&path)
                .unwrap_or_else(|e| panic!("actions failed for {bundle}: {e}"));
            for action in &actions {
                assert!(
                    action.uuid.as_deref().is_some_and(|u| !u.is_empty()),
                    "action without uuid in {bundle}"
                );
                assert!(
                    action.name.as_deref().is_some_and(|n| !n.trim().is_empty()),
                    "action without name in {bundle}: {:?}",
                    action.uuid
                );
            }
        }
    }
}
