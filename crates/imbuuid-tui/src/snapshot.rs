// file_id: SOM-DOC-10434 (part)
// name: src/snapshot.rs
// description: [Snap] (F5) — writes current view state + full store snapshot
//   to ~/.imbuuid/snapshots/<unix-ts>.json.
// project_id: IMBUUID-MEM
// category: tui
// created: 2026-10-17
// version: 1.0.0

use crate::app::{App, SortMode};
use crate::config::Density;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct ViewState {
    query: String,
    sort: String,
    density: String,
    selected_uuid: Option<String>,
    filtered_count: usize,
    search_focused: bool,
}

#[derive(Serialize)]
struct Snapshot {
    app: String,
    unix_ts: u64,
    view: ViewState,
    memories: Vec<SnapshotMemory>,
}

#[derive(Serialize)]
struct SnapshotMemory {
    uuid: String,
    content: String,
    namespace_label: String,
    namespace_raw: u16,
    observation_count: u64,
    signal: f32,
    first_seen_sec: u64,
    last_seen_sec: u64,
    #[serde(serialize_with = "hash_hex")]
    content_hash: u64,
    prompt_id: Option<String>,
    #[serde(rename = "type")]
    type_name: String,
    provenance: String,
    fractal_dgd: String,
    fractal_sector_raw: u16,
}

fn hash_hex<S: serde::Serializer>(h: &u64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&format!("{:016x}", h))
}

fn sort_label(s: &SortMode) -> &'static str {
    match s {
        SortMode::ObsDesc => "obs-desc",
        SortMode::ObsAsc => "obs-asc",
        SortMode::LastSeenDesc => "last-seen-desc",
    }
}

fn density_label(d: &Density) -> &'static str {
    d.label()
}

/// Write the snapshot; returns the file path on success.
pub fn write_snapshot(app: &App) -> Result<PathBuf, String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = app.base_dir.join("snapshots");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", ts));

    let snap = Snapshot {
        app: "imbuuid-tui".to_string(),
        unix_ts: ts,
        view: ViewState {
            query: app.query.clone(),
            sort: sort_label(&app.sort).to_string(),
            density: density_label(&app.density).to_string(),
            selected_uuid: app.selected_view().map(|v| v.uuid.clone()),
            filtered_count: app.filtered.len(),
            search_focused: app.search_focused,
        },
        memories: app
            .views
            .iter()
            .map(|v| SnapshotMemory {
                uuid: v.uuid.clone(),
                content: v.content.clone(),
                namespace_label: v.namespace_label.clone(),
                namespace_raw: v.namespace_raw,
                observation_count: v.observation_count,
                signal: v.signal,
                first_seen_sec: v.first_seen_sec,
                last_seen_sec: v.last_seen_sec,
                content_hash: v.content_hash,
                prompt_id: v.prompt_id.clone(),
                type_name: v.type_name.clone(),
                provenance: v.provenance_name.clone(),
                fractal_dgd: v.fractal_dgd.clone(),
                fractal_sector_raw: v.fractal_sector_raw,
            })
            .collect(),
    };

    let raw = serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    tracing::info!(path = %path.display(), memories = snap.memories.len(), "snapshot written");
    Ok(path)
}

#[allow(dead_code)]
pub fn snapshots_dir(base: &Path) -> PathBuf {
    base.join("snapshots")
}
