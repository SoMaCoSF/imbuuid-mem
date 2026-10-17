// file_id: SOM-DOC-10434 (part)
// name: src/logging.rs
// description: tracing_subscriber wired to ~/.imbuuid/tui.log (file ONLY —
//   the TUI owns the screen; nothing ever logs to stdout/stderr).
// project_id: IMBUUID-MEM
// category: tui
// created: 2026-10-17
// version: 1.0.0

use std::fs::{create_dir_all, OpenOptions};
use std::path::Path;
use std::sync::Mutex;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

/// Initialize file logging. Idempotent-ish: repeated calls are ignored via
/// `try_init` so tests and re-entry don't panic.
pub fn init(base: &Path) -> Result<(), String> {
    create_dir_all(base).map_err(|e| format!("create {}: {}", base.display(), e))?;
    let log_path = base.join("tui.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("open {}: {}", log_path.display(), e))?;
    let writer = BoxMakeWriter::new(Mutex::new(file));
    let _ = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .try_init();
    tracing::info!(path = %log_path.display(), "tui.log initialized");
    Ok(())
}
