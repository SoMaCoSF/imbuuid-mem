// SOM-DOC-10435 (part) — contextual dossier: compile → preview → publish.
//
// Per SOM-DOC-10439: compile walks UUID-keyed memory records
// chronologically, receipt-backed, no invented entries, gaps shown as gaps.
// Media attach from ~/.imbuuid/captures + ~/.imbuuid/snapshots at capture
// timestamps. Preview renders in-panel; PUBLISH is an explicit-choice
// dialog every time (Gist / repo docs/dossier/<slug>.md / cancel).
// No silent network, ever: Gist posts ONLY via `gh gist create` after the
// operator confirms, otherwise the payload lands in the outbox.

use crate::browser::fmt_utc;
use crate::store::StoreSnapshot;
use chrono::{TimeZone, Utc};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Time gap between consecutive records that renders as an explicit gap.
const GAP_THRESHOLD_SEC: u64 = 6 * 3600;

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One record-backed dossier entry. Every field comes from the store;
/// nothing is generated from vibes.
#[derive(Debug, Clone)]
pub struct DossierEntry {
    pub uuid: String,
    pub ts_utc: String,
    pub namespace: String,
    pub signal: f64,
    pub summary: String,
    pub content_hash: String,
    pub attestation_count: u32,
    pub receipt_lines: Vec<String>,
    pub prompt_links: Vec<String>,
    pub unattested: bool,
}

#[derive(Debug, Clone)]
pub struct MediaRef {
    pub path: String,
    pub kind: &'static str, // "capture" | "snapshot"
    pub ts_sec: u64,
    pub ts_utc: String,
}

#[derive(Debug, Clone)]
pub struct DossierDraft {
    pub slug: String,
    pub title: String,
    pub scope_label: String,
    pub generated_utc: String,
    pub entries: Vec<DossierEntry>,
    pub gaps: Vec<String>,
    pub media: Vec<MediaRef>,
}

/// Capture/snaps attached at their capture timestamps, if present.
pub fn scan_media() -> Vec<MediaRef> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut out = Vec::new();
    for (dir, kind) in [("captures", "capture"), ("snapshots", "snapshot")] {
        let p = Path::new(&home).join(".imbuuid").join(dir);
        let rd = match std::fs::read_dir(&p) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            let ts = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.push(MediaRef {
                path: e.path().display().to_string(),
                kind,
                ts_sec: ts,
                ts_utc: fmt_utc(ts),
            });
        }
    }
    out.sort_by_key(|m| m.ts_sec);
    tracing::info!(media = out.len(), "dossier.scan_media");
    out
}

/// Compile the draft from the current store scope. Chronological,
/// receipt-backed; gaps in the record show as gaps, not prose.
pub fn compile_dossier(store: &dyn StoreSnapshot, scope_label: &str) -> DossierDraft {
    let now = now_sec();
    let mut records: Vec<_> = store.memories().iter().collect();
    records.sort_by_key(|m| m.created_sec);

    let mut entries = Vec::new();
    let mut gaps = Vec::new();
    let mut prev: Option<u64> = None;
    for m in records {
        if let Some(p) = prev {
            if m.created_sec > p && m.created_sec - p > GAP_THRESHOLD_SEC {
                gaps.push(format!(
                    "gap of {}h between {} and {} — no records in this window",
                    (m.created_sec - p) / 3600,
                    fmt_utc(p),
                    fmt_utc(m.created_sec)
                ));
            }
        }
        prev = Some(m.created_sec);
        let atts = store.attestations_for(&m.uuid);
        let receipt_lines: Vec<String> = atts
            .iter()
            .map(|a| format!("#{} {} {} {}", a.seq, a.witness, a.verdict, fmt_utc(a.ts_sec)))
            .collect();
        let prompt_links: Vec<String> = store
            .prompt_fingerprints()
            .iter()
            .filter(|pr| pr.linked_memory_uuids.iter().any(|u| u == &m.uuid))
            .map(|pr| pr.uuid.clone())
            .collect();
        entries.push(DossierEntry {
            uuid: m.uuid.clone(),
            ts_utc: fmt_utc(m.created_sec),
            namespace: m.namespace.clone(),
            signal: m.signal,
            summary: m.summary.clone(),
            content_hash: m.content_hash.clone(),
            attestation_count: m.attestations,
            unattested: receipt_lines.is_empty(),
            receipt_lines,
            prompt_links,
        });
    }

    let media = scan_media();
    let slug = format!("dossier-{}", now);
    tracing::info!(
        entries = entries.len(),
        gaps = gaps.len(),
        media = media.len(),
        "dossier.compile"
    );
    DossierDraft {
        slug,
        title: format!("Dossier — {} ({} entries)", scope_label, entries.len()),
        scope_label: scope_label.to_string(),
        generated_utc: Utc
            .timestamp_opt(now as i64, 0)
            .single()
            .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "?".to_string()),
        entries,
        gaps,
        media,
    }
}

fn ghost_header(draft: &DossierDraft) -> String {
    let date = draft
        .generated_utc
        .split(' ')
        .next()
        .unwrap_or("2026-10-17")
        .to_string();
    format!(
        "<!--\n\
         ===============================================================================\n\
         file_id: SOM-DOC-10435-v1.0.0 (part: dossier)\n\
         name: {slug}.md\n\
         description: Contextual dossier compiled from UUID-keyed memory records —\n\
           receipt-backed, no invented entries. Scope: {scope}.\n\
         project_id: IMBUUID-MEM\n\
         category: dossier\n\
         tags: [dossier, memories, receipts]\n\
         created: {date}\n\
         version: 1.0.0\n\
         ===============================================================================\n\
         -->\n",
        slug = draft.slug,
        scope = draft.scope_label,
        date = date
    )
}

fn short_uuid(u: &str) -> String {
    format!("{:.8}…", u)
}

/// Render the publish payload. Docs standard: Ghost header + ≥1 mermaid.
pub fn render_markdown(draft: &DossierDraft) -> String {
    let mut md = ghost_header(draft);
    md.push_str(&format!("# {}\n\n", draft.title));
    md.push_str(&format!(
        "Scope: `{}` · Generated: {} · Entries: {} · Gaps: {} · Media: {}\n\n",
        draft.scope_label,
        draft.generated_utc,
        draft.entries.len(),
        draft.gaps.len(),
        draft.media.len()
    ));
    md.push_str("Receipt-backed compilation: every entry below cites a UUID-keyed\n");
    md.push_str("memory record and its witness receipts. Gaps in the record are\n");
    md.push_str("shown as gaps, not prose. Nothing is invented.\n\n");

    // --- mermaid: record timeline ---
    md.push_str("## Record timeline\n\n```mermaid\nflowchart LR\n");
    let shown = draft.entries.iter().take(12).collect::<Vec<_>>();
    for (i, e) in shown.iter().enumerate() {
        md.push_str(&format!(
            "    E{i}[\"{u}<br/>{ts}\"]\n",
            i = i,
            u = short_uuid(&e.uuid),
            ts = e.ts_utc
        ));
    }
    for i in 1..shown.len() {
        md.push_str(&format!("    E{} --> E{}\n", i - 1, i));
    }
    if draft.entries.len() > shown.len() {
        md.push_str(&format!(
            "    E{} --> MORE[\"+{} more\"]\n",
            shown.len() - 1,
            draft.entries.len() - shown.len()
        ));
    }
    md.push_str("```\n\n");

    // --- records ---
    md.push_str("## Records (chronological)\n\n");
    for (i, e) in draft.entries.iter().enumerate() {
        md.push_str(&format!("### {}. `{}`\n\n", i + 1, e.uuid));
        md.push_str(&format!(
            "- timestamp: {} · namespace: `{}` · signal: {:.4}\n",
            e.ts_utc, e.namespace, e.signal
        ));
        md.push_str(&format!("- content-hash: `{}`\n", e.content_hash));
        md.push_str(&format!(
            "- attestations: {}{}\n",
            e.attestation_count,
            if e.unattested {
                " — UNATTESTED (no witness receipts on record)"
            } else {
                ""
            }
        ));
        if !e.receipt_lines.is_empty() {
            md.push_str("- receipts:\n");
            for r in &e.receipt_lines {
                md.push_str(&format!("  - `{}`\n", r));
            }
        }
        if !e.prompt_links.is_empty() {
            md.push_str(&format!("- linked prompts: {}\n", e.prompt_links.join(", ")));
        }
        md.push_str(&format!("\n{}\n\n", e.summary));
    }

    // --- gaps ---
    md.push_str("## Gaps\n\n");
    if draft.gaps.is_empty() {
        md.push_str("No gaps — the record is continuous within the 6-hour threshold.\n\n");
    } else {
        for g in &draft.gaps {
            md.push_str(&format!("- {}\n", g));
        }
        md.push('\n');
    }

    // --- media ---
    md.push_str("## Media\n\n");
    if draft.media.is_empty() {
        md.push_str("No captures or snapshots present at compile time.\n\n");
    } else {
        for m in &draft.media {
            md.push_str(&format!(
                "- [{}] `{}` — captured {}\n",
                m.kind, m.path, m.ts_utc
            ));
        }
        md.push('\n');
    }

    md.push_str("---\n*Compiled by imbuuid-panel from the local UUID-keyed store. \
        The UUID is the address; this dossier cites what the store holds.*\n");
    md
}

/// The PUBLISH step verifies the docs standard: Ghost header + mermaid.
/// Refuses payloads that lack either.
pub fn validate_dossier_payload(payload: &str) -> Result<(), String> {
    if !payload.contains("file_id:") {
        return Err("missing Ghost Catalog header (file_id:)".to_string());
    }
    if !payload.contains("```mermaid") {
        return Err("missing mermaid diagram (docs standard)".to_string());
    }
    Ok(())
}

fn outbox_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("IMBUUID_DOSSIER_OUTBOX") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".imbuuid").join("dossier-outbox")
}

fn outbox_path(slug: &str) -> PathBuf {
    outbox_dir().join(format!("{}.md", slug))
}

/// What a publish attempt did. Gist is operator-tapped upstream; this
/// function is only ever called from the explicit-choice dialog.
#[derive(Debug, Clone)]
pub enum PublishReport {
    PostedGist { url: String },
    WroteRepo { path: PathBuf },
    WroteOutbox { path: PathBuf, reason: String },
}

impl PublishReport {
    pub fn describe(&self) -> String {
        match self {
            PublishReport::PostedGist { url } => format!("posted GitHub Gist → {}", url),
            PublishReport::WroteRepo { path } => {
                format!("wrote repo dossier → {}", path.display())
            }
            PublishReport::WroteOutbox { path, reason } => {
                format!("outbox → {} ({})", path.display(), reason)
            }
        }
    }
}

fn gh_available() -> bool {
    Command::new("gh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Gist post — ONLY called after the operator explicitly chose "Gist" in
/// the publish dialog. Posts via `gh gist create`; on any failure (or when
/// `gh` is absent, or IMBUUID_DOSSIER_NO_GIST=1) the payload goes to the
/// outbox and the report says exactly what happened. No silent network.
pub fn publish_gist_or_outbox(slug: &str, payload: &str) -> PublishReport {
    if let Err(e) = validate_dossier_payload(payload) {
        tracing::warn!("dossier payload invalid, refusing gist: {}", e);
        return write_outbox(slug, payload, &format!("payload invalid, refused: {}", e));
    }
    if std::env::var("IMBUUID_DOSSIER_NO_GIST").as_deref() == Ok("1") {
        return write_outbox(slug, payload, "IMBUUID_DOSSIER_NO_GIST=1 (no network)");
    }
    if !gh_available() {
        tracing::info!("gh not available — dossier to outbox");
        return write_outbox(slug, payload, "gh not available");
    }
    // Stage the payload as <slug>.md so the gist file carries the slug name.
    let stage = std::env::temp_dir().join(format!("imbuuid-dossier-{}", slug));
    let file = stage.join(format!("{}.md", slug));
    if let Err(e) = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&stage)?;
        std::fs::write(&file, payload)?;
        Ok(())
    })() {
        return write_outbox(slug, payload, &format!("staging failed: {}", e));
    }
    let out = Command::new("gh")
        .arg("gist")
        .arg("create")
        .arg("--public=false")
        .arg("--desc")
        .arg(format!("imbuuid dossier {}", slug))
        .arg(&file)
        .output();
    let _ = std::fs::remove_dir_all(&stage);
    match out {
        Ok(o) if o.status.success() => {
            let url = String::from_utf8_lossy(&o.stdout).trim().to_string();
            tracing::info!(url = %url, "dossier.gist_posted");
            PublishReport::PostedGist { url }
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
            tracing::warn!("gh gist create failed: {}", err);
            write_outbox(slug, payload, &format!("gh gist create failed: {}", err))
        }
        Err(e) => write_outbox(slug, payload, &format!("gh exec failed: {}", e)),
    }
}

fn write_outbox(slug: &str, payload: &str, reason: &str) -> PublishReport {
    let path = outbox_path(slug);
    match (|| -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, payload)?;
        Ok(())
    })() {
        Ok(()) => {
            tracing::info!(path = %path.display(), reason = %reason, "dossier.outbox");
            PublishReport::WroteOutbox {
                path,
                reason: reason.to_string(),
            }
        }
        Err(e) => {
            tracing::error!("outbox write failed: {}", e);
            PublishReport::WroteOutbox {
                path,
                reason: format!("{}; outbox write ALSO failed: {}", reason, e),
            }
        }
    }
}

/// Write `docs/dossier/<slug>.md` under the target repo root. Creates dirs.
/// The repo root comes from IMBUUID_DOSSIER_REPO (operator's environment),
/// defaulting to the current directory — the operator chose this branch in
/// the same explicit dialog, so the destination is never a silent default.
pub fn publish_to_repo(
    repo_root: &str,
    slug: &str,
    payload: &str,
) -> Result<PublishReport, String> {
    validate_dossier_payload(payload)?;
    let dir = Path::new(repo_root).join("docs").join("dossier");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;
    let path = dir.join(format!("{}.md", slug));
    std::fs::write(&path, payload).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
    tracing::info!(path = %path.display(), "dossier.repo_write");
    Ok(PublishReport::WroteRepo { path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryStore;

    fn valid_payload() -> String {
        let store = InMemoryStore::seeded();
        let draft = compile_dossier(&store, "test scope");
        render_markdown(&draft)
    }

    #[test]
    fn dossier_compiles_only_record_backed_entries() {
        let store = InMemoryStore::seeded();
        let draft = compile_dossier(&store, "test scope");
        assert_eq!(draft.entries.len(), store.memories().len());
        // Every entry cites a real record UUID — no invented entries.
        let known: std::collections::HashSet<String> =
            store.memories().iter().map(|m| m.uuid.clone()).collect();
        for e in &draft.entries {
            assert!(known.contains(&e.uuid), "entry not backed by a record: {}", e.uuid);
        }
        // Chronological: entry order matches records sorted by created_sec.
        let mut sorted: Vec<&crate::store::MemoryEntry> = store.memories().iter().collect();
        sorted.sort_by_key(|m| m.created_sec);
        let entry_uuids: Vec<&str> = draft.entries.iter().map(|e| e.uuid.as_str()).collect();
        let sorted_uuids: Vec<&str> = sorted.iter().map(|m| m.uuid.as_str()).collect();
        assert_eq!(entry_uuids, sorted_uuids);
    }

    #[test]
    fn dossier_gaps_shown_as_gaps() {
        let mut store = InMemoryStore::seeded();
        // Force a >6h gap between two records.
        store.memories[0].created_sec = 1_000_000;
        store.memories[1].created_sec = 1_000_000 + 10 * 3600;
        let draft = compile_dossier(&store, "test");
        assert!(!draft.gaps.is_empty(), "expected a gap annotation");
        assert!(draft.gaps[0].contains("gap of 10h"));
    }

    #[test]
    fn rendered_payload_carries_header_and_mermaid() {
        let payload = valid_payload();
        assert!(payload.contains("file_id: SOM-DOC-10435-v1.0.0 (part: dossier)"));
        assert!(payload.contains("```mermaid"));
        assert!(payload.contains("flowchart LR"));
        validate_dossier_payload(&payload).unwrap();
    }

    #[test]
    fn validate_refuses_headerless_or_mermaidless() {
        assert!(validate_dossier_payload("# no header").is_err());
        assert!(validate_dossier_payload("file_id: x\nno mermaid").is_err());
    }

    #[test]
    fn publish_never_posts_without_explicit_confirm() {
        // The outbox path is what runs when the operator does NOT confirm a
        // gist post (or gh is unavailable): no Command::new("gh") is spawned.
        std::env::set_var("IMBUUID_DOSSIER_NO_GIST", "1");
        let dir = std::env::temp_dir().join("imbuuid-dossier-test-outbox");
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("IMBUUID_DOSSIER_OUTBOX", dir.to_str().unwrap());
        let payload = valid_payload();
        let report = publish_gist_or_outbox("test-slug", &payload);
        assert!(!matches!(report, PublishReport::PostedGist { .. }), "must not post a gist: {:?}", report);
        match &report {
            PublishReport::WroteOutbox { path, reason } => {
                assert!(path.exists(), "outbox file must exist");
                assert_eq!(std::fs::read_to_string(path).unwrap(), payload);
                assert!(reason.contains("NO_GIST"));
            }
            other => panic!("expected outbox, got {:?}", other),
        }
        std::env::remove_var("IMBUUID_DOSSIER_NO_GIST");
        std::env::remove_var("IMBUUID_DOSSIER_OUTBOX");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_payload_goes_to_outbox_not_network() {
        std::env::set_var("IMBUUID_DOSSIER_NO_GIST", "1");
        let dir = std::env::temp_dir().join("imbuuid-dossier-test-outbox2");
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("IMBUUID_DOSSIER_OUTBOX", dir.to_str().unwrap());
        let report = publish_gist_or_outbox("bad-slug", "# no header, no mermaid");
        assert!(!matches!(report, PublishReport::PostedGist { .. }));
        std::env::remove_var("IMBUUID_DOSSIER_NO_GIST");
        std::env::remove_var("IMBUUID_DOSSIER_OUTBOX");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn publish_to_repo_writes_docs_dossier() {
        let dir = std::env::temp_dir().join("imbuuid-dossier-test-repo");
        let _ = std::fs::remove_dir_all(&dir);
        let payload = valid_payload();
        let report = publish_to_repo(dir.to_str().unwrap(), "test-slug", &payload).unwrap();
        let path = match &report {
            PublishReport::WroteRepo { path } => path.clone(),
            other => panic!("expected WroteRepo, got {:?}", other),
        };
        assert_eq!(
            path,
            dir.join("docs").join("dossier").join("test-slug.md")
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), payload);
        assert!(publish_to_repo(dir.to_str().unwrap(), "bad", "no header").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn media_scan_handles_missing_dirs() {
        // Must not error when ~/.imbuuid/captures is absent.
        let media = scan_media();
        for m in &media {
            assert!(m.path.contains(".imbuuid"));
        }
    }
}
