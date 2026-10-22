//! Log file support.
//!
//! The app normally only logs to stdout, which a packaged desktop app does not
//! have. A small `MakeWriter` appends the same events to
//! `<data dir>/logs/integratedeck.log` (rotated once it grows past 5 MiB), so a
//! user can hand over a log when something goes wrong.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::MakeWriter;

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Clone)]
pub struct LogFileMaker {
    file: Arc<Mutex<File>>,
}

impl LogFileMaker {
    /// Open (and rotate when oversized) the log file.
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::metadata(path)
            .map(|meta| meta.len() > MAX_LOG_BYTES)
            .unwrap_or(false)
        {
            let rotated = path.with_extension("log.1");
            let _ = std::fs::rename(path, rotated);
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
        })
    }
}

pub struct LogFileGuard<'a> {
    file: std::sync::MutexGuard<'a, File>,
}

impl Write for LogFileGuard<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl<'a> MakeWriter<'a> for LogFileMaker {
    type Writer = LogFileGuard<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        // A poisoned lock only means a previous writer panicked; the file handle
        // itself is still usable.
        let file = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        LogFileGuard { file }
    }
}
