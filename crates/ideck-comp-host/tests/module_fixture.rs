//! End-to-end Companion module fixture (no external dependencies).

use ideck_comp_host::CompModuleRuntime;
use serde_json::json;

fn write_fixture(root: &std::path::Path) {
    let module = root.join("demo");
    std::fs::create_dir_all(module.join("dist")).unwrap();
    std::fs::write(module.join("package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(
        module.join("dist/index.js"),
        r#"
export default class Demo {
  constructor(context) {
    this.context = context
  }
  getConfigFields() {
    return [{ id: 'host', type: 'textinput', label: 'Host', default: '127.0.0.1' }]
  }
  async init(config, isFirst, secrets) {
    this.context.setActionDefinitions({
      act1: {
        name: 'Act 1',
        options: [],
        callback: async () => {
          this.context.setVariableValues({ state: 'acted' })
        },
      },
      act2: {
        name: 'Act 2',
        options: [],
        callback: async (action) => {
          this.context.setVariableValues({ state: String(action.options?.value ?? 'none') })
        },
      }
    })
    this.context.setVariableDefinitions({ state: { name: 'State' } })
    this.context.setVariableValues({ state: 'ready' })
    this.context.setPresetDefinitions({ p1: { type: 'button', name: 'Preset 1' } })
    this.context.setFeedbackDefinitions({
      fb1: {
        name: 'FB 1',
        type: 'boolean',
        options: [],
        callback: async (feedback) => {
          this.context.setVariableValues({ state: String(feedback.options?.value ?? 'fb') })
          return { color: 0x00ff00, text: 'fb' }
        },
      },
    })
  }
  async configUpdated(config, secrets) {
    this.context.setVariableValues({ state: String(secrets?.token ?? 'updated') })
  }
  async destroy() {}
}
"#,
    )
    .unwrap();
}

#[tokio::test]
async fn module_fixture_roundtrip() {
    let root = std::env::temp_dir().join(format!("ideck-comp-fixture-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    write_fixture(&root);

    let runtime = CompModuleRuntime::spawn(&root).await.expect("spawn");
    let added = runtime
        .request(
            "connection.add",
            json!({ "id": "c1", "moduleId": "demo", "label": "Demo", "config": {} }),
        )
        .await
        .expect("add");
    assert!(added.ok, "connection.add failed: {:?}", added.error);

    let defs = runtime
        .request("connection.getDefinitions", json!({ "connectionId": "c1" }))
        .await
        .expect("defs")
        .result
        .unwrap();
    eprintln!("defs={defs:?}");
    assert_eq!(defs["actions"].as_array().unwrap().len(), 2);
    assert_eq!(defs["variables"].as_array().unwrap().len(), 1);
    assert_eq!(defs["presets"].as_array().unwrap().len(), 1);

    let fields = runtime
        .request(
            "connection.getConfigFields",
            json!({ "connectionId": "c1" }),
        )
        .await
        .expect("fields")
        .result
        .unwrap();
    assert_eq!(fields["fields"][0]["id"], "host");

    let executed = runtime
        .request(
            "connection.executeAction",
            json!({ "connectionId": "c1", "actionId": "act1", "options": {} }),
        )
        .await
        .expect("execute");
    assert!(executed.ok, "execute failed: {:?}", executed.error);

    let acted = runtime
        .request(
            "connection.getVariableValue",
            json!({ "connectionId": "c1", "variableId": "state" }),
        )
        .await
        .expect("var")
        .result
        .unwrap();
    assert_eq!(acted["value"], "acted");

    let fields = runtime
        .request(
            "connection.getConfigFields",
            json!({ "connectionId": "c1" }),
        )
        .await
        .expect("fields")
        .result
        .unwrap();
    assert_eq!(fields["fields"][0]["id"], "host");

    let with_options = runtime
        .request(
            "connection.executeAction",
            json!({
                "connectionId": "c1",
                "actionId": "act2",
                "options": { "value": "opt" }
            }),
        )
        .await
        .expect("execute act2");
    assert!(with_options.ok, "act2 failed: {:?}", with_options.error);

    let opted = runtime
        .request(
            "connection.getVariableValue",
            json!({ "connectionId": "c1", "variableId": "state" }),
        )
        .await
        .expect("var")
        .result
        .unwrap();
    assert_eq!(opted["value"], "opt");

    let updated = runtime
        .request(
            "connection.updateConfig",
            json!({
                "connectionId": "c1",
                "config": { "host": "10.0.0.2" },
                "secrets": { "token": "s3" }
            }),
        )
        .await
        .expect("update");
    assert!(updated.ok, "updateConfig failed: {:?}", updated.error);

    let value = runtime
        .request(
            "connection.getVariableValue",
            json!({ "connectionId": "c1", "variableId": "state" }),
        )
        .await
        .expect("var")
        .result
        .unwrap();
    assert_eq!(value["value"], "s3");

    let feedback = runtime
        .request(
            "connection.setFeedback",
            json!({
                "connectionId": "c1",
                "controlId": "c1:act1",
                "feedbackId": "fb1",
                "options": { "value": "fb-opt" }
            }),
        )
        .await
        .expect("setFeedback");
    assert!(feedback.ok, "setFeedback failed: {:?}", feedback.error);

    let feedback_value = runtime
        .request(
            "connection.getVariableValue",
            json!({ "connectionId": "c1", "variableId": "state" }),
        )
        .await
        .expect("var")
        .result
        .unwrap();
    assert_eq!(feedback_value["value"], "fb-opt");

    let removed = runtime
        .request("connection.remove", json!({ "connectionId": "c1" }))
        .await
        .expect("remove");
    assert!(removed.ok);

    let _ = std::fs::remove_dir_all(&root);
}
