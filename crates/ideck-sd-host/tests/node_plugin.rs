//! Verifies that a Node SDK plugin is launched with the official app args.

use std::path::Path;
use std::time::Duration;

use ideck_sd_host::{spawn_node, PluginLaunchStrategy, StreamDeckManifest};

#[tokio::test]
async fn node_plugin_launch_passes_official_args() {
    // Unique per run: a leftover directory from an interrupted run must not be
    // mistaken for this run's output.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("ideck-node-plugin-{}-{unique}", std::process::id()));
    let dir = root.join("com.example.node.sdPlugin");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        r#"{
            "Name": "Node Demo",
            "UUID": "com.example.node",
            "Version": "1.0.0",
            "Nodejs": { "Version": "20" },
            "CodePath": "args.js",
            "Actions": []
        }"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("args.js"),
        "require('node:fs').writeFileSync('args.json', JSON.stringify(process.argv))",
    )
    .unwrap();

    let manifest = StreamDeckManifest::load(&dir).unwrap();
    let strategy = PluginLaunchStrategy::detect(&manifest, &dir).unwrap();
    assert!(
        matches!(strategy, PluginLaunchStrategy::NodeScript { .. }),
        "expected NodeScript, got {strategy:?}"
    );

    let mut child = spawn_node(
        Path::new("args.js"),
        &dir,
        12345,
        "com.example.node",
        "registerPlugin",
        r#"{"plugin":{"uuid":"com.example.node"}}"#,
    )
    .await
    .expect("spawn node");

    let args_path = dir.join("args.json");
    let mut args = String::new();
    // Node cold start on a loaded CI host can take several seconds.
    for _ in 0..150 {
        if let Ok(text) = std::fs::read_to_string(&args_path) {
            args = text;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = child.start_kill();

    assert!(
        args.contains("args.js"),
        "node plugin did not write argv within 15s (argv: {args:?}, dir: {})",
        dir.display()
    );
    assert!(args.contains("-port"), "argv: {args}");
    assert!(args.contains("12345"), "argv: {args}");
    assert!(args.contains("-pluginUUID"), "argv: {args}");
    assert!(args.contains("com.example.node"), "argv: {args}");
    assert!(args.contains("-registerEvent"), "argv: {args}");

    let _ = std::fs::remove_dir_all(&root);
}
