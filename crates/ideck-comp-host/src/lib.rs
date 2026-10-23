//! Companion module compatibility host — embedded Boa, no Node.js.
//!
//! Runs third-party `@companion-module/base` plugins through integratedeck's own
//! compatibility layer, not Bitfocus Companion or `@companion-module/host`.

mod connections;
mod engine;
mod protocol;
mod registry;
mod runtime;

pub use connections::*;
pub use protocol::*;
pub use registry::*;
pub use runtime::*;
