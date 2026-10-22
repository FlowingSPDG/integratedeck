//! Launch-at-login registration.
//!
//! - Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
//! - macOS: a LaunchAgent plist in `~/Library/LaunchAgents`
//! - Linux: a `.desktop` file in `~/.config/autostart`
//!
//! The settings toggle is only a promise unless the OS entry exists, so the app
//! applies it on save and re-syncs it on startup.

#[cfg(target_os = "windows")]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const VALUE_NAME: &str = "integratedeck";

#[cfg(target_os = "windows")]
mod imp {
    use super::{RUN_KEY, VALUE_NAME};

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

    pub fn is_enabled() -> bool {
        std::process::Command::new("reg")
            .args(["query", RUN_KEY, "/v", VALUE_NAME])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;

    const LABEL: &str = "app.integratedeck.desktop";

    fn plist_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/LaunchAgents")
                .join(format!("{LABEL}.plist")),
        )
    }

    /// LaunchAgent plist that starts the app at login.
    pub fn plist_contents(exe: &std::path::Path) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
</dict>
</plist>
"#,
            exe = exe.display()
        )
    }

    pub fn apply(enabled: bool) -> Result<(), String> {
        let Some(path) = plist_path() else {
            return Err("HOME が取得できません".into());
        };
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, plist_contents(&exe)).map_err(|e| e.to_string())?;
        } else if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn is_enabled() -> bool {
        plist_path().map(|path| path.exists()).unwrap_or(false)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::path::PathBuf;

    fn desktop_path() -> Option<PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(config.join("autostart").join("integratedeck.desktop"))
    }

    /// XDG autostart entry.
    pub fn desktop_contents(exe: &std::path::Path) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=integratedeck\nExec=\"{exe}\"\nX-GNOME-Autostart-enabled=true\nTerminal=false\n",
            exe = exe.display()
        )
    }

    pub fn apply(enabled: bool) -> Result<(), String> {
        let Some(path) = desktop_path() else {
            return Err("HOME が取得できません".into());
        };
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, desktop_contents(&exe)).map_err(|e| e.to_string())?;
        } else if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn is_enabled() -> bool {
        desktop_path().map(|path| path.exists()).unwrap_or(false)
    }
}

/// Enable or disable starting the app when the user logs in.
pub fn apply(enabled: bool) -> Result<(), String> {
    imp::apply(enabled)
}

/// Whether the app is currently registered to start at login.
pub fn is_enabled() -> bool {
    imp::is_enabled()
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::imp::plist_contents;

    #[test]
    fn plist_names_the_executable_and_runs_at_load() {
        let text = plist_contents(std::path::Path::new("/Applications/integratedeck.app"));
        assert!(text.contains("<string>app.integratedeck.desktop</string>"));
        assert!(text.contains("<string>/Applications/integratedeck.app</string>"));
        assert!(text.contains("<key>RunAtLoad</key>"));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::imp::desktop_contents;

    #[test]
    fn desktop_entry_quotes_the_executable() {
        let text = desktop_contents(std::path::Path::new("/usr/bin/integratedeck"));
        assert!(text.contains("Type=Application"));
        assert!(text.contains("Exec=\"/usr/bin/integratedeck\""));
    }
}
