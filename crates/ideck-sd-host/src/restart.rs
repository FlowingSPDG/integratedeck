use std::path::Path;
use std::time::Duration;

use tokio::process::Child;
use tracing::warn;

/// Restart a crashed plugin process with exponential backoff.
pub async fn restart_with_backoff<F, Fut>(
    mut spawn: F,
    max_attempts: u32,
) -> Result<Child, anyhow::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Child, anyhow::Error>>,
{
    let mut delay = Duration::from_secs(1);
    for attempt in 1..=max_attempts {
        match spawn().await {
            Ok(child) => return Ok(child),
            Err(e) if attempt < max_attempts => {
                warn!("plugin spawn failed (attempt {attempt}): {e}, retry in {delay:?}");
                tokio::time::sleep(delay).await;
                delay = delay.saturating_mul(2);
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

pub fn discover_sd_plugins(dir: &Path) -> Vec<std::path::PathBuf> {
    crate::scan::scan_sd_plugins(dir)
}
