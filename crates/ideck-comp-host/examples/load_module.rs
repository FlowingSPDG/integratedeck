//! Load a Companion module and print its definitions.
//!
//! ```text
//! cargo run -p ideck-comp-host --example load_module -- <modules-root> <module-id>
//! ```

use std::path::PathBuf;

use ideck_comp_host::CompModuleRuntime;
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().expect("modules root"));
    let module_id = args.next().expect("module id");

    let runtime = CompModuleRuntime::spawn(&root).await?;
    let added = runtime
        .request(
            "connection.add",
            json!({
                "id": "smoke",
                "moduleId": module_id,
                "label": module_id,
                "config": {},
            }),
        )
        .await?;
    println!("add ok={} error={:?}", added.ok, added.error);

    let defs = runtime
        .request(
            "connection.getDefinitions",
            json!({ "connectionId": "smoke" }),
        )
        .await?;
    println!("definitions ok={} error={:?}", defs.ok, defs.error);
    if let Some(result) = defs.result {
        let actions = result["actions"].as_array().map_or(0, Vec::len);
        let feedbacks = result["feedbacks"].as_array().map_or(0, Vec::len);
        let variables = result["variables"].as_array().map_or(0, Vec::len);
        let presets = result["presets"].as_array().map_or(0, Vec::len);
        println!("actions={actions} feedbacks={feedbacks} variables={variables} presets={presets}");
    }

    let removed = runtime
        .request("connection.remove", json!({ "connectionId": "smoke" }))
        .await?;
    println!("remove ok={} error={:?}", removed.ok, removed.error);
    Ok(())
}
