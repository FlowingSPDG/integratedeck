//! The Companion sidecar must not outlive the app: Node children are not reaped
//! by the OS, so `shutdown` has to kill the process and drop its temp script.

use ideck_comp_host::EmbeddedEngine;

#[tokio::test]
async fn shutdown_kills_the_sidecar_and_removes_its_script() {
    let modules = std::env::temp_dir().join(format!("ideck-sidecar-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&modules);
    std::fs::create_dir_all(&modules).unwrap();

    let mut engine = EmbeddedEngine::spawn(&modules).expect("spawn sidecar");
    let script = engine.script_path().to_path_buf();
    assert!(
        script.is_file(),
        "sidecar script should exist while running"
    );
    assert!(engine.is_running().await, "sidecar should be running");

    // ping proves the node process is actually serving requests
    let pong = tokio::time::timeout(std::time::Duration::from_secs(30), engine.ping())
        .await
        .expect("ping timed out")
        .expect("ping failed");
    assert!(pong.is_object() || pong.is_null());

    engine.shutdown().await;

    let mut stopped = false;
    for _ in 0..50 {
        if !engine.is_running().await {
            stopped = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(stopped, "sidecar process still running after shutdown");
    assert!(!script.exists(), "sidecar script not removed");

    let _ = std::fs::remove_dir_all(&modules);
}
