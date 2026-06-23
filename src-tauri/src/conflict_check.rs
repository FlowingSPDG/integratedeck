use serde::Serialize;
use sysinfo::{ProcessesToUpdate, System};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictingApp {
    pub id: String,
    pub display_name: String,
    pub process_name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupConflictReport {
    pub conflicts: Vec<ConflictingApp>,
}

pub fn detect_conflicting_apps() -> StartupConflictReport {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let current_pid = std::process::id();
    let mut stream_deck: Option<ConflictingApp> = None;
    let mut companion: Option<ConflictingApp> = None;

    for (pid, process) in system.processes() {
        if pid.as_u32() == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().into_owned();
        let normalized = normalize_process_name(&name);

        if stream_deck.is_none() && is_stream_deck_process(&normalized) {
            stream_deck = Some(ConflictingApp {
                id: "stream_deck".into(),
                display_name: "Elgato Stream Deck".into(),
                process_name: name.clone(),
            });
        }

        if companion.is_none() && is_companion_process(&normalized) {
            companion = Some(ConflictingApp {
                id: "companion".into(),
                display_name: "Bitfocus Companion".into(),
                process_name: name,
            });
        }

        if stream_deck.is_some() && companion.is_some() {
            break;
        }
    }

    let mut conflicts = Vec::new();
    if let Some(entry) = stream_deck {
        conflicts.push(entry);
    }
    if let Some(entry) = companion {
        conflicts.push(entry);
    }

    StartupConflictReport { conflicts }
}

fn normalize_process_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn is_stream_deck_process(normalized: &str) -> bool {
    normalized == "stream deck"
        || normalized == "streamdeck"
        || normalized == "elgato stream deck"
        || normalized.starts_with("stream deck ")
}

fn is_companion_process(normalized: &str) -> bool {
    normalized == "companion"
        || normalized.starts_with("companion ")
        || normalized.starts_with("companion-")
            && !normalized.contains("integratedeck")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_stream_deck_process_names() {
        assert!(is_stream_deck_process("stream deck"));
        assert!(is_stream_deck_process("streamdeck"));
        assert!(is_stream_deck_process("elgato stream deck"));
        assert!(!is_stream_deck_process("integratedeck"));
    }

    #[test]
    fn detects_companion_process_names() {
        assert!(is_companion_process("companion"));
        assert!(is_companion_process("companion-headless"));
        assert!(!is_companion_process("integratedeck"));
    }
}
