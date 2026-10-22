//! Snapshot of running executables, used for Elgato-style "smart profiles":
//! a profile can list applications that switch to it while they run.

use std::collections::HashSet;

/// Lowercase executable names of every running process (with `.exe` on Windows).
#[cfg(target_os = "windows")]
pub fn running_executables() -> HashSet<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut names = HashSet::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return names;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snapshot, &mut entry);
        while ok != 0 {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if !name.is_empty() {
                names.insert(name.to_ascii_lowercase());
            }
            ok = Process32NextW(snapshot, &mut entry);
        }
        let _ = CloseHandle(snapshot);
    }
    names
}

/// `/proc`-based fallback for the other platforms.
#[cfg(not(target_os = "windows"))]
pub fn running_executables() -> HashSet<String> {
    let mut names = HashSet::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return names;
    };
    for entry in entries.flatten() {
        let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) else {
            continue;
        };
        let name = comm.trim().to_ascii_lowercase();
        if !name.is_empty() {
            names.insert(name);
        }
    }
    names
}

/// Whether `application` (as typed by the user) is running.
///
/// Accepts names with or without `.exe` and ignores case, so `Notepad.exe`,
/// `notepad.exe` and `notepad` all match.
pub fn application_running(application: &str, running: &HashSet<String>) -> bool {
    let needle = application.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return false;
    }
    let with_exe = if needle.ends_with(".exe") {
        needle.clone()
    } else {
        format!("{needle}.exe")
    };
    running.contains(&needle) || running.contains(&with_exe)
}

/// The profile that should be active given the running processes.
///
/// Returns the first profile (in name order) whose `applications` list matches.
pub fn matching_profile<'a>(
    profiles: &'a [(String, String, Vec<String>)],
    running: &HashSet<String>,
) -> Option<&'a str> {
    profiles
        .iter()
        .filter(|(_, _, applications)| {
            applications
                .iter()
                .any(|application| application_running(application, running))
        })
        .map(|(id, _, _)| id.as_str())
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn matches_with_and_without_exe_and_ignores_case() {
        let running = set(&["notepad.exe", "code.exe"]);
        assert!(application_running("Notepad", &running));
        assert!(application_running("notepad.exe", &running));
        assert!(application_running(" CODE.EXE ", &running));
        assert!(!application_running("photoshop", &running));
        assert!(!application_running("   ", &running));
    }

    #[test]
    fn picks_the_first_profile_with_a_running_app() {
        let profiles = vec![
            (
                "a".to_string(),
                "Alpha".to_string(),
                vec!["obs64".to_string()],
            ),
            (
                "b".to_string(),
                "Beta".to_string(),
                vec!["code".to_string()],
            ),
        ];
        assert_eq!(matching_profile(&profiles, &set(&["code.exe"])), Some("b"));
        assert_eq!(matching_profile(&profiles, &set(&["explorer.exe"])), None);
    }
}
