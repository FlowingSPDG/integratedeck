//! Bootstrap/API smoke tests for the embedded Companion host.

use ideck_comp_host::CompModuleRuntime;
use serde_json::json;

#[tokio::test]
async fn ping_and_method_dispatch() {
    let dir = std::env::temp_dir().join(format!("ideck-comp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let runtime = CompModuleRuntime::spawn(&dir).await.expect("spawn");
    let pong = runtime.ping().await.expect("ping");
    assert_eq!(pong["engine"], "node-sidecar");

    let defs = runtime
        .request(
            "connection.getDefinitions",
            json!({ "connectionId": "missing" }),
        )
        .await
        .expect("request");
    assert!(defs.ok);
    let defs = defs.result.unwrap();
    assert_eq!(defs["actions"], json!([]));
    assert_eq!(defs["presets"], json!([]));

    let config_fields = runtime
        .request(
            "connection.getConfigFields",
            json!({ "connectionId": "missing" }),
        )
        .await
        .expect("request");
    assert!(!config_fields.ok, "unknown connection should error");

    let _ = std::fs::remove_dir_all(&dir);
}
