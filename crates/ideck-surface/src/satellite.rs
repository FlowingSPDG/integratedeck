//! Satellite remote surface protocol (stub for Phase 5).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteConfig {
    pub host: String,
    pub port: u16,
}

impl Default for SatelliteConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 16622,
        }
    }
}

/// Placeholder for TCP satellite surface connection.
pub struct SatelliteClient {
    pub config: SatelliteConfig,
}

impl SatelliteClient {
    pub fn new(config: SatelliteConfig) -> Self {
        Self { config }
    }

    pub async fn connect(&self) -> Result<(), String> {
        // Full protocol in future phase
        Err("Satellite protocol not yet implemented".into())
    }
}
