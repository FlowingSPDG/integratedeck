//! Smoke test against real Elgato plugins installed on the machine.
//!
//! Ignored by default; run with:
//! `cargo test -p ideck-sd-host --test real_plugin -- --ignored --nocapture`

use std::path::PathBuf;
use std::time::Duration;

use ideck_sd_host::{BrokerEvent, StreamDeckBroker};
use serde_json::json;

fn plugins_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("STREAMDECK_PLUGIN_PATH") {
        return Some(PathBuf::from(path));
    }
    if let Ok(dir) = std::env::var("STREAMDECK_PLUGIN_DIR") {
        return Some(PathBuf::from(dir));
    }
    let appdata = std::env::var("APPDATA").ok()?;
    Some(
        PathBuf::from(appdata)
            .join("Elgato")
            .join("StreamDeck")
            .join("Plugins"),
    )
}

fn first_plugin(path: &std::path::Path) -> Option<PathBuf> {
    if path.is_dir() && path.extension().and_then(|e| e.to_str()) == Some("sdPlugin") {
        return Some(path.to_path_buf());
    }
    let mut entries: Vec<_> = std::fs::read_dir(path).ok()?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    entries.into_iter().map(|e| e.path()).find(|p| {
        p.is_dir()
            && p.extension().and_then(|e| e.to_str()) == Some("sdPlugin")
            && (p.join("manifest.json").exists()
                || p.join("Code").exists()
                || p.join("code").exists())
    })
}

#[tokio::test]
#[ignore = "requires locally installed Stream Deck plugins"]
async fn real_elgato_plugin_registers() {
    let Some(root) = plugins_dir() else {
        eprintln!("no plugin dir");
        return;
    };
    let Some(plugin) = first_plugin(&root) else {
        eprintln!("no .sdPlugin under {}", root.display());
        return;
    };
    eprintln!("loading {}", plugin.display());

    let devices = json!([{
        "id": "ideck-test-device",
        "name": "integratedeck test",
        "size": { "columns": 5, "rows": 3 },
        "type": 0
    }]);

    let (_broker, mut events) = tokio::time::timeout(
        Duration::from_secs(15),
        StreamDeckBroker::start(&plugin, devices),
    )
    .await
    .expect("broker start timed out")
    .expect("broker start");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            panic!("plugin did not register within 20s");
        }
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Some(BrokerEvent::PluginRegistered { plugin_uuid })) => {
                eprintln!("registered {plugin_uuid}");
                return;
            }
            Ok(Some(other)) => eprintln!("event: {other:?}"),
            Ok(None) => panic!("broker event channel closed"),
            Err(_) => panic!("plugin did not register within 20s"),
        }
    }
}
