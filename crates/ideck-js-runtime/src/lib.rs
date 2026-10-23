//! Embedded Boa runtime shared by Companion and Stream Deck plugin hosts.

mod engine;
mod loader;
mod resolver;
mod shims;
mod ws;

pub use engine::{JsEngine, JsEngineConfig};
pub use loader::IdeckModuleLoader;
pub use resolver::{resolve_specifier, url_to_path};
pub use shims::{install_globals, install_require};
pub use ws::{install_websocket, WsBridge};
