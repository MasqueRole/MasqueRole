//! Optional debug log file. Everything written passes through [`ir_core::redact`], so API
//! keys never reach the disk.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ir_core::redact::redact;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::Layer as _;

/// Name of the log file inside the data directory.
pub const LOG_FILE: &str = "masquerole.log";

/// A log that grows beyond this many bytes is moved aside at the next start.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// A writer that redacts secrets from everything passing through.
struct RedactingWriter<W>(W);

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // The formatter hands over one whole event per call, so a secret is never split.
        let text = redact(&String::from_utf8_lossy(buf));
        self.0.write_all(text.as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

fn open_log(path: &Path) -> io::Result<File> {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        // Best effort: if the move fails, appending to the big file still works.
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// Start logging to `dir/`[`LOG_FILE`] when `enabled`. Returns the log path, or `None` when
/// logging is off or the file cannot be opened (the app runs fine without a log).
///
/// Panics are always routed to the log (when it is on) and then reported as usual.
pub fn init(enabled: bool, dir: &Path) -> Option<PathBuf> {
    if !enabled {
        return None;
    }
    let path = dir.join(LOG_FILE);
    let file = std::fs::create_dir_all(dir)
        .and_then(|()| open_log(&path))
        .map_err(|e| eprintln!("could not open the log file {}: {e}", path.display()))
        .ok()?;
    // Only this app's own crates: HTTP and GUI libraries are too noisy at debug level.
    let only_ours = Targets::new()
        .with_target("ir_core", tracing::Level::DEBUG)
        .with_target("ir_app", tracing::Level::DEBUG)
        .with_target("ir_ui", tracing::Level::DEBUG);
    let layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(Mutex::new(RedactingWriter(file)))
        .with_filter(only_ours);
    if tracing_subscriber::registry()
        .with(layer)
        .try_init()
        .is_err()
    {
        return None;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        previous(info);
    }));
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "logging started");
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_writer_removes_secrets_and_reports_the_full_length() {
        ir_core::redact::register_secret("sk-logging-test-key");
        let mut out = Vec::new();
        let mut writer = RedactingWriter(&mut out);
        let line = b"sent with sk-logging-test-key ok\n";
        assert_eq!(writer.write(line).unwrap(), line.len());
        assert_eq!(String::from_utf8(out).unwrap(), "sent with [redacted] ok\n");
    }

    #[test]
    fn nothing_is_created_while_logging_is_off() {
        let dir = std::env::temp_dir().join("ir-logging-off-test");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(init(false, &dir), None);
        assert!(!dir.exists());
    }

    #[test]
    fn a_big_log_is_moved_aside() {
        let dir = std::env::temp_dir().join(format!("ir-logging-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(LOG_FILE);
        std::fs::write(&path, vec![b'x'; (MAX_LOG_BYTES + 1) as usize]).unwrap();
        drop(open_log(&path).unwrap());
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        assert!(dir.join("masquerole.log.old").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
