// file_id: SOM-DOC-10434 (part)
// name: src/backend.rs
// description: wire-up seam between the TUI and imbuuid-mem — the ONLY module
//   that touches the crate's API, now wired against Crew 1's REAL
//   implementation (gyst + memory + witness + prompt + retrieval +
//   provenance all landed 2026-10-17). Real API notes that matter here:
//   - MemoryStore::open(Option<PathBuf>) takes a DIRECTORY; the v0 snapshot
//     lives at <dir>/memories.json. insert(content: &str, opts: InsertOpts).
//   - MemoryRecord.namespace is the u16 fnv1a12 hash, not the label — the
//     TUI keeps a label registry (labels it wrote itself) with hex fallback.
//   - Witness is in-memory (no persistence in v0): new()/attest(&InsertOutcome,
//     ts)/attestations()/verify_chain(). Kinds: FirstObservation/ReObservation.
//     The TUI re-observes the store once per session so the witness chain
//     and title-bar count are meaningful within the session.
//   - fingerprint_prompt(text, provenance) -> TYPE_PROMPT UUID (ts pinned 0).
//   - retrieve(store, query, &QueryOpts) -> Vec<ScoredMemory{uuid, score,
//     observation_count}> — resolve records via store.get(&uuid).
//   - binary_provenance() -> Option<BinaryProvenance{uuid, source_tree_hash,
//     timestamp_sec, builder}> from build-time env.
// project_id: IMBUUID-MEM
// category: tui
// created: 2026-10-17
// version: 1.0.0

use imbuuid_mem::gyst::{self, DecodedGyst};
use imbuuid_mem::memory::{InsertOpts, InsertOutcome, MemoryRecord, MemoryStore};
use imbuuid_mem::prompt::{fingerprint_prompt, link_prompt_memory};
use imbuuid_mem::provenance::binary_provenance;
use imbuuid_mem::retrieval::{retrieve, QueryOpts};
use imbuuid_mem::witness::Witness;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Flat, UI-owned view of a memory record. The TUI never reaches past this.
#[derive(Debug, Clone)]
pub struct MemoryView {
    pub uuid: String,
    pub content: String,
    pub namespace_label: String,
    pub namespace_raw: u16,
    pub observation_count: u64,
    pub signal: f32,
    pub first_seen_sec: u64,
    pub last_seen_sec: u64,
    pub content_hash: u64,
    pub prompt_id: Option<String>,
    pub type_name: String,
    pub provenance_name: String,
    pub fractal_dgd: String,
    pub fractal_sector_raw: u16,
}

impl MemoryView {
    /// First 8 hex chars of the UUID (hyphens stripped).
    pub fn uuid_short(&self) -> String {
        self.uuid.chars().filter(|c| *c != '-').take(8).collect()
    }

    /// Content preview clamped to `n` chars on char boundaries.
    pub fn preview(&self, n: usize) -> String {
        let flat: String = self
            .content
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        if flat.chars().count() <= n {
            flat
        } else {
            let mut s: String = flat.chars().take(n.saturating_sub(1)).collect();
            s.push('…');
            s
        }
    }
}

/// Flat view of an attestation receipt.
#[derive(Debug, Clone)]
pub struct AttestationView {
    pub receipt: String,
    pub kind: String,
    pub ts: u64,
    pub observation_count: u64,
}

pub fn base_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".imbuuid")
}

/// Display path of the v0 snapshot (the store itself opens the directory).
pub fn snapshot_path(base: &Path) -> PathBuf {
    base.join("memories.json")
}

pub fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn open_store(dir: &Path) -> Result<MemoryStore, String> {
    MemoryStore::open(Some(dir.to_path_buf()))
        .map_err(|e| format!("open store dir {}: {}", dir.display(), e))
}

pub fn new_witness() -> Witness {
    Witness::new()
}

/// Resolve a namespace hash to its label via the registry; hex fallback for
/// hashes the TUI never wrote (the record only carries the u16).
pub fn namespace_label(raw: u16, registry: &HashMap<u16, String>) -> String {
    registry
        .get(&raw)
        .cloned()
        .unwrap_or_else(|| format!("ns:{:#06x}", raw))
}

pub fn register_label(registry: &mut HashMap<u16, String>, label: &str) {
    registry.insert(gyst::fnv1a12(label), label.to_string());
}

pub fn type_name(code: u16) -> String {
    match code {
        gyst::TYPE_MEMORY => "memory".to_string(),
        gyst::TYPE_PROMPT => "prompt".to_string(),
        gyst::TYPE_ATTESTATION => "attestation".to_string(),
        gyst::TYPE_BINARY_PROVENANCE => "binary-provenance".to_string(),
        gyst::TYPE_TURBO_KV => "turbo-kv".to_string(),
        gyst::TYPE_TURBO_ATTN => "turbo-attn".to_string(),
        gyst::TYPE_TURBO_LAYER => "turbo-layer".to_string(),
        gyst::TYPE_POLY_MARKET => "poly-market".to_string(),
        gyst::TYPE_POLY_OUTCOME_QUOTE => "poly-outcome-quote".to_string(),
        other => format!("type:{:#x}", other),
    }
}

pub fn provenance_name(code: u8) -> String {
    match code {
        gyst::PROV_UNKNOWN => "unknown".to_string(),
        gyst::PROV_DEXTER => "dexter".to_string(),
        gyst::PROV_CLI => "cli".to_string(),
        gyst::PROV_CLAUDE => "claude".to_string(),
        gyst::PROV_DASHBOARD => "dashboard".to_string(),
        gyst::PROV_REGISTRY => "registry".to_string(),
        gyst::PROV_AGENT => "agent".to_string(),
        gyst::PROV_POLY_MAKER => "poly-maker".to_string(),
        gyst::PROV_TURBO_QUANT => "turbo-quant".to_string(),
        gyst::PROV_ENCRYPTED => "encrypted".to_string(),
        other => format!("prov:{:#x}", other),
    }
}

fn fractal_dgd_of(d: &DecodedGyst) -> String {
    format!(
        "d{}/dom{}/g{}",
        d.fractal_depth, d.fractal_domain, d.fractal_generation
    )
}

pub fn view_of(rec: &MemoryRecord, registry: &HashMap<u16, String>) -> MemoryView {
    let (type_name, fractal_dgd) = match gyst::decode_gyst(&rec.uuid) {
        Ok(d) => (type_name(d.type_code), fractal_dgd_of(&d)),
        Err(_) => ("undecodable".to_string(), "—".to_string()),
    };
    MemoryView {
        uuid: rec.uuid.clone(),
        content: rec.content.clone(),
        namespace_label: namespace_label(rec.namespace, registry),
        namespace_raw: rec.namespace,
        observation_count: rec.observation_count,
        signal: rec.signal,
        first_seen_sec: rec.first_seen_sec,
        last_seen_sec: rec.last_seen_sec,
        content_hash: rec.content_hash,
        prompt_id: rec.prompt_id.clone(),
        type_name,
        provenance_name: provenance_name(rec.provenance),
        fractal_dgd,
        fractal_sector_raw: rec.fractal_sector,
    }
}

pub fn load_views(
    store: &MemoryStore,
    registry: &HashMap<u16, String>,
) -> Vec<MemoryView> {
    let mut v: Vec<MemoryView> = store.iter().map(|r| view_of(r, registry)).collect();
    // Deterministic order: HashMap iteration is not.
    v.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    v
}

pub fn attestations_for(witness: &Witness, uuid: &str) -> Vec<AttestationView> {
    let mut v: Vec<AttestationView> = witness
        .attestations()
        .iter()
        .filter(|a| a.uuid == uuid)
        .map(|a| AttestationView {
            receipt: a.receipt.clone(),
            kind: a.kind.as_str().to_string(),
            ts: a.timestamp_sec,
            observation_count: a.observation_count,
        })
        .collect();
    v.sort_by_key(|a| a.ts);
    v
}

pub fn attestation_count(witness: &Witness) -> usize {
    witness.attestations().len()
}

/// Session re-observation: the v0 witness is in-memory, so each TUI session
/// witnesses the records it loads (as ReObservation). Makes the title-bar
/// count and detail-pane receipts meaningful within the session.
pub fn reobserve(store: &MemoryStore, witness: &mut Witness) {
    let ts = now_sec();
    for rec in store.iter() {
        witness.attest(
            &InsertOutcome::Duplicate {
                uuid: rec.uuid.clone(),
                observation_count: rec.observation_count,
            },
            ts,
        );
    }
    debug_assert!(witness.verify_chain());
    tracing::info!(attestations = witness.attestations().len(), "session re-observation");
}

/// Related memories via the crate's retrieval plane (excludes the record
/// itself). Wide sector tolerance: rank by signal + frequency, don't prefilter.
pub fn related(
    store: &MemoryStore,
    view: &MemoryView,
    registry: &HashMap<u16, String>,
    top_k: usize,
) -> Vec<(MemoryView, f32)> {
    let mut q = QueryOpts::new();
    q.top_k = top_k + 1;
    q.sector_tolerance_bits = 12;
    retrieve(store, &view.content, &q)
        .into_iter()
        .filter(|s| s.uuid != view.uuid)
        .take(top_k)
        .filter_map(|s| {
            store
                .get(&s.uuid)
                .map(|r| (view_of(r, registry), s.score))
        })
        .collect()
}

/// (uuid, tree_hash, timestamp_sec, builder) from the provenance plane.
pub fn provenance_info() -> Option<(String, String, u64, String)> {
    binary_provenance().map(|p| (p.uuid, p.source_tree_hash, p.timestamp_sec, p.builder))
}

/// Short (8-hex) provenance UUID for the title bar, or "unprovenanced".
pub fn provenance_short() -> String {
    match provenance_info() {
        Some((uuid, _, _, _)) => uuid.chars().filter(|c| *c != '-').take(8).collect(),
        None => "unprovenanced".to_string(),
    }
}

/// Seed the demo corpus. Returns number of samples processed.
pub fn seed_demo(
    store: &mut MemoryStore,
    witness: &mut Witness,
    registry: &mut HashMap<u16, String>,
) -> Result<usize, String> {
    let mut n = 0;
    for s in crate::demo::samples() {
        register_label(registry, s.namespace);
        let prompt_uuid = s.prompt.map(|p| fingerprint_prompt(p, gyst::PROV_CLI));
        let mut opts = InsertOpts::new(s.namespace);
        opts.signal = s.signal;
        opts.prompt_id = prompt_uuid.clone();
        let outcome = store
            .insert(s.content, opts)
            .map_err(|e| format!("demo insert: {}", e))?;
        let uuid = match &outcome {
            InsertOutcome::Created(rec) => rec.uuid.clone(),
            InsertOutcome::Duplicate { uuid, .. } => uuid.clone(),
        };
        if let Some(pu) = prompt_uuid {
            let link = link_prompt_memory(&pu, &uuid);
            tracing::debug!(prompt_uuid = %link.prompt_uuid, memory_uuid = %link.memory_uuid, "demo prompt link");
        }
        if s.attest {
            witness.attest(&outcome, now_sec());
        }
        n += 1;
    }
    debug_assert!(witness.verify_chain());
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_view() -> MemoryView {
        MemoryView {
            uuid: String::new(),
            content: String::new(),
            namespace_label: String::new(),
            namespace_raw: 0,
            observation_count: 0,
            signal: 0.0,
            first_seen_sec: 0,
            last_seen_sec: 0,
            content_hash: 0,
            prompt_id: None,
            type_name: String::new(),
            provenance_name: String::new(),
            fractal_dgd: String::new(),
            fractal_sector_raw: 0,
        }
    }

    fn registry() -> HashMap<u16, String> {
        let mut r = HashMap::new();
        register_label(&mut r, "ops");
        r
    }

    #[test]
    fn uuid_short_strips_hyphens_and_takes_8() {
        let mut v = blank_view();
        v.uuid = "4005aae7-7800-8000-89ff-feaaaaaaaaab".to_string();
        assert_eq!(v.uuid_short(), "4005aae7");
    }

    #[test]
    fn preview_truncates_on_char_boundary() {
        let mut v = blank_view();
        v.content = "héllo wörld, this is a long memory content string".to_string();
        let p = v.preview(10);
        assert!(p.chars().count() <= 10, "got {:?}", p);
        assert!(p.ends_with('…'));
        v.content = "short".to_string();
        assert_eq!(v.preview(40), "short");
    }

    #[test]
    fn type_and_provenance_names_cover_contract_consts() {
        assert_eq!(type_name(gyst::TYPE_MEMORY), "memory");
        assert_eq!(type_name(gyst::TYPE_BINARY_PROVENANCE), "binary-provenance");
        assert_eq!(provenance_name(gyst::PROV_CLI), "cli");
        assert_eq!(provenance_name(gyst::PROV_ENCRYPTED), "encrypted");
        assert!(type_name(0x999).starts_with("type:"));
    }

    #[test]
    fn namespace_registry_resolves_and_falls_back() {
        let r = registry();
        assert_eq!(namespace_label(gyst::fnv1a12("ops"), &r), "ops");
        assert!(namespace_label(0x123, &r).starts_with("ns:"));
    }

    #[test]
    fn view_of_real_record_decodes() {
        let mut store = MemoryStore::open(None).unwrap();
        let mut opts = InsertOpts::new("ops");
        opts.signal = 0.75;
        let uuid = match store.insert("real record", opts).unwrap() {
            InsertOutcome::Created(rec) => rec.uuid.clone(),
            _ => panic!("expected Created"),
        };
        let rec = store.get(&uuid).unwrap();
        let v = view_of(rec, &registry());
        assert_eq!(v.type_name, "memory");
        assert_eq!(v.provenance_name, "cli");
        assert_eq!(v.namespace_label, "ops");
        assert_eq!(v.observation_count, 1);
        assert!((v.signal - 0.75).abs() < 1e-6);
        assert_eq!(v.uuid_short().len(), 8);
    }

    #[test]
    fn seed_demo_inserts_25_with_attestations_and_persists() {
        let dir = std::env::temp_dir().join(format!(
            "imbuuid-tui-seedtest-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = open_store(&dir).unwrap();
        let mut witness = new_witness();
        let mut reg = HashMap::new();
        let n = seed_demo(&mut store, &mut witness, &mut reg).unwrap();
        assert_eq!(n, 25);
        assert_eq!(store.len(), 25);
        assert!(attestation_count(&witness) > 0, "demo should attest some memories");
        assert!(witness.verify_chain());
        // Reload from disk: the v0 snapshot round-trips.
        let store2 = open_store(&dir).unwrap();
        assert_eq!(store2.len(), 25);
        let views = load_views(&store2, &reg);
        assert!(views.iter().all(|v| v.type_name == "memory"));
        assert!(views.iter().any(|v| v.prompt_id.is_some()));
        let labels: std::collections::HashSet<&str> =
            views.iter().map(|v| v.namespace_label.as_str()).collect();
        assert!(labels.contains("ops") && labels.contains("research"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
