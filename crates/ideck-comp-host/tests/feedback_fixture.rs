//! Feedback event round-trip through the Node sidecar.

use std::time::Duration;

use ideck_comp_host::CompModuleRuntime;
use serde_json::json;

fn write_fixture(root: &std::path::Path) {
    let module = root.join("fb");
    std::fs::create_dir_all(module.join("dist")).unwrap();
    std::fs::write(module.join("package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(
        module.join("dist/index.js"),
        r#"
export default class FbDemo {
  constructor(context) { this.context = context }
  getConfigFields() { return [] }
  async init() {
    this.context.setActionDefinitions({
      act1: { name: 'Act', options: [], callback: async () => {} }
    })
    this.context.setFeedbackDefinitions({
      act1: {
        name: 'FB',
        type: 'advanced',
        options: [],
        callback: async () => ({ bgcolor: '#ff0000', text: 'FB' })
      }
    })
    setTimeout(() => { void this.context.checkFeedbacks() }, 25)
  }
  async destroy() {}
}
"#,
    )
    .unwrap();
}

#[tokio::test]
async fn feedback_update_reaches_event_stream() {
    let root = std::env::temp_dir().join(format!("ideck-comp-fb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    write_fixture(&root);

    let runtime = CompModuleRuntime::spawn(&root).await.expect("spawn");
    let mut events = runtime.subscribe_events();
    let added = runtime
        .request(
            "connection.add",
            json!({ "id": "fb1", "moduleId": "fb", "config": {} }),
        )
        .await
        .expect("add");
    assert!(added.ok, "add failed: {:?}", added.error);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "no feedback.updated event");
        let event = tokio::time::timeout(remaining, events.recv())
            .await
            .expect("event timeout")
            .expect("event channel");
        if event.event != "feedback.updated" {
            continue;
        }
        let values = event.data["values"].as_array().cloned().unwrap_or_default();
        assert_eq!(values[0]["style"]["text"], "FB");
        assert_eq!(values[0]["style"]["bgcolor"], "#ff0000");
        break;
    }

    let _ = runtime
        .request("connection.remove", json!({ "connectionId": "fb1" }))
        .await;
    let _ = std::fs::remove_dir_all(&root);
}
