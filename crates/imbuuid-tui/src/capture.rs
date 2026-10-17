// file_id: SOM-DOC-10434 (part)
// name: src/capture.rs
// description: [Capture] (F7) — writes the current rendered frame as plain
//   text to ~/.imbuuid/captures/<unix-ts>.txt (a text screenshot).
// project_id: IMBUUID-MEM
// category: tui
// created: 2026-10-17
// version: 1.0.0

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Write captured frame text; returns the file path on success.
pub fn write_capture(base: &std::path::Path, frame_text: &str) -> Result<PathBuf, String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = base.join("captures");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.txt", ts));
    let header = format!(
        "imbuuid-tui frame capture — unix_ts={} — text screenshot ({} lines)\n{}\n",
        ts,
        frame_text.lines().count(),
        "=".repeat(72)
    );
    std::fs::write(&path, format!("{}{}", header, frame_text)).map_err(|e| e.to_string())?;
    tracing::info!(path = %path.display(), "frame captured");
    Ok(path)
}
