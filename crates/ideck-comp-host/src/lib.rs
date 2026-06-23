//! Companion module host — orchestrator-managed Node process (stdio IPC).

mod connections;
mod protocol;
mod registry;
mod runtime;

pub use connections::*;
pub use protocol::*;
pub use registry::*;
pub use runtime::*;
