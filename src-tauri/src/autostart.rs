//! Launch-at-login registration.
//!
//! Windows keeps it in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`;
//! the settings toggle is only a promise unless the value is written, so the
//! app applies it on save and re-syncs it on startup.

#[cfg(target_os = "windows")]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const VALUE_NAME: &str = "integratedeck";

/// Enable or disable starting the app when the user logs in.
#[cfg(target_os = "windows")]
pub fn apply(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if enabled {
        let value = format!("\"{}\"", exe.display());
        let output = std::process::Command::new("reg")
            .args([
                "add", RUN_KEY, "/v", VALUE_NAME, "/t", "REG_SZ", "/d", &value, "/f",
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                "スタートアップ登録に失敗しました".into()
            } else {
                stderr
            });
        }
    } else {
        // Removing a value that does not exist exits non-zero, which is fine.
        let _ = std::process::Command::new("reg")
            .args(["delete", RUN_KEY, "/v", VALUE_NAME, "/f"])
            .output()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Whether the app is currently registered to start at login.
#[cfg(target_os = "windows")]
pub fn is_enabled() -> bool {
    std::process::Command::new("reg")
        .args(["query", RUN_KEY, "/v", VALUE_NAME])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
pub fn apply(_enabled: bool) -> Result<(), String> {
    tracing::debug!("launch-at-login is not implemented on this platform");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn is_enabled() -> bool {
    false
}
