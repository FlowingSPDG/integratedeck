use std::path::PathBuf;

/// Path to Node script that hosts HTML-based SD plugins (CodePath: *.html).
pub fn html_host_script() -> PathBuf {
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/sd-html-host.mjs");
    if dev.exists() {
        return dev;
    }
    PathBuf::from("scripts/sd-html-host.mjs")
}
