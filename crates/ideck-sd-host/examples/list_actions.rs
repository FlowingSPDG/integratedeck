//! Print the actions a Stream Deck plugin declares, including which controllers
//! (`Keypad` / `Encoder`) each action supports.
//!
//! Usage: `cargo run -p ideck-sd-host --example list_actions -- <plugin.sdPlugin>`

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: list_actions <plugin.sdPlugin>");
        std::process::exit(2);
    };
    let dir = std::path::PathBuf::from(path);
    let summary = ideck_sd_host::StreamDeckManifest::read_summary(&dir);
    match &summary {
        Some(summary) => println!("plugin: {} ({:?})", summary.name, summary.uuid),
        None => println!("plugin: <unreadable>"),
    }
    match ideck_sd_host::StreamDeckManifest::list_actions(&dir) {
        Ok(actions) => {
            println!("actions: {}", actions.len());
            for action in actions {
                let id = action.uuid.as_deref().unwrap_or("<no uuid>");
                let name = action.name.as_deref().unwrap_or("<no name>");
                let controllers = action
                    .controllers
                    .as_ref()
                    .map(|list| list.join("+"))
                    .unwrap_or_else(|| "-".into());
                println!(
                    "  {id} | {name} | controllers={controllers} | encoder={} keypad={}",
                    action.supports_encoder(),
                    action.supports_keypad()
                );
            }
        }
        Err(e) => {
            eprintln!("failed to list actions: {e}");
            std::process::exit(1);
        }
    }
}
