// SOM-DOC-10435 (part) — panel data layer.
//
// INTEGRATION SEAM (open): the panel renders through the `StoreSnapshot`
// trait. `InMemoryStore` is the built-in source: deterministic seeded demo
// data, plus a JSON snapshot loader (`--snapshot <path>` /
// `IMBUUID_PANEL_SNAPSHOT`). Wiring Crew 1's live imbuuid-mem store in means
// implementing `StoreSnapshot` for their store type — no UI changes needed.
// The seeded data is clearly labelled DEMO in the chrome.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// One memory row in the list / detail panes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub uuid: String,
    pub namespace: String,
    pub content_hash: String,
    pub signal: f64,
    pub attestations: u32,
    pub created_sec: u64,
    pub summary: String,
}

/// One witness attestation for the live feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    pub seq: u64,
    pub witness: String,
    pub memory_uuid: String,
    pub verdict: String,
    pub ts_sec: u64,
}

/// A prompt fingerprint the panel knows about: the TYPE_PROMPT UUID plus
/// the memory UUIDs it was linked to. Carries no prompt text (privacy).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRef {
    pub uuid: String,
    pub provenance: u8,
    pub linked_memory_uuids: Vec<String>,
}

/// Aggregate numbers for the stats tile.
#[derive(Debug, Clone)]
pub struct StoreStats {
    pub total_memories: u64,
    pub total_attestations: u64,
    pub dedup_collapses: u64,
    pub namespaces: Vec<(String, u64)>,
}

/// Anything the panel can render. Implement for the live store later.
pub trait StoreSnapshot {
    fn stats(&self) -> StoreStats;
    fn attestations(&self) -> &[Attestation];
    fn memories(&self) -> &[MemoryEntry];
    /// Newest-first feed, capped at `n`.
    fn feed(&self, n: usize) -> Vec<&Attestation> {
        let a = self.attestations();
        let start = a.len().saturating_sub(n);
        a[start..].iter().rev().collect()
    }

    // ---- crew4 resolution seam (defaulted: existing impls don't break) ----
    /// Find a memory record by exact UUID.
    fn lookup_memory(&self, uuid: &str) -> Option<&MemoryEntry> {
        self.memories().iter().find(|m| m.uuid == uuid)
    }
    /// Attestations referencing this UUID.
    fn attestations_for(&self, uuid: &str) -> Vec<&Attestation> {
        self.attestations()
            .iter()
            .filter(|a| a.memory_uuid == uuid)
            .collect()
    }
    /// Known prompt fingerprints (default: none).
    fn prompt_fingerprints(&self) -> Vec<&PromptRef> {
        Vec::new()
    }
    /// Memory UUIDs linked from a prompt fingerprint (default: via
    /// prompt_fingerprints, so overriding the one above is enough).
    fn prompt_links(&self, prompt_uuid: &str) -> Vec<&str> {
        let mut out = Vec::new();
        for p in self.prompt_fingerprints() {
            if p.uuid == prompt_uuid {
                out.extend(p.linked_memory_uuids.iter().map(|s| s.as_str()));
            }
        }
        out
    }
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1_760_700_000)
}

/// In-memory snapshot: seeded demo data, JSON-loadable, live-tickable.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InMemoryStore {
    pub memories: Vec<MemoryEntry>,
    pub attestations: Vec<Attestation>,
    pub dedup_collapses: u64,
    #[serde(default)]
    pub demo: bool,
    /// Prompt fingerprints known to the panel (browser source).
    #[serde(default)]
    pub prompts: Vec<PromptRef>,
}

impl InMemoryStore {
    /// Deterministic demo seed. Timestamps anchor at load time so the feed
    /// always looks alive; UUIDs are real TYPE_MEMORY v8 mints (via
    /// imbuuid-mem's gyst codec) so the UUID browser decodes them honestly.
    pub fn seeded() -> Self {
        let now = now_sec();
        let ns = ["agent:planner", "agent:researcher", "witness:ledger"];
        let summaries = [
            "Prefers terse status lines over paragraphs; acknowledged 2026-10-14.",
            "Kalshi doctrine: signals+PAPER only, no live-trading code path.",
            "Gumroad gating: binaries ride Gumroad only, updater notify-only.",
            "UUID substrate: every record UUID-keyed, receipts on transitions.",
            "Harvest absorption ruling 2026-10-14: clean licensed code folds in.",
            "Identity repo is per-GitHub-user, not per-install (reversed 2026-10-17).",
            "Hex-panel grammar: unit hex + stretched hex, nothing else.",
            ".bk law: backup before every destructive edit.",
            "Delivery convention: deployed link + localhost URL, both 200-verified.",
            "STOP/NO is an absolute halt; report touched-vs-untouched.",
            "Claim boundary: never 'per Hitachi specs'; working model, not the plan.",
            "REA loop: acquire, exercise, measure, extract, license-gate, absorb, log.",
        ];
        let memories: Vec<MemoryEntry> = summaries
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let created = now - (i as u64 * 1400 + 300);
                let ns_label = ns[i % ns.len()];
                let signal = 0.55 + (i as f64 * 0.037) % 0.45;
                MemoryEntry {
                    uuid: imbuuid_mem::gyst::encode_gyst(
                        imbuuid_mem::gyst::TYPE_MEMORY,
                        imbuuid_mem::gyst::fnv1a12(ns_label),
                        created,
                        (i % 16) as u8,
                        ((i / 4) % 16) as u8,
                        0,
                        signal as f32,
                        imbuuid_mem::gyst::PROV_CLI,
                        0xabc000 + i as u64 * 0x1111,
                    ),
                    namespace: ns_label.to_string(),
                    content_hash: format!("{:064x}", 0xdeadbeef_u64.wrapping_add(i as u64 * 0x9e37)),
                    signal,
                    attestations: 2 + (i as u32 % 5),
                    created_sec: created,
                    summary: s.to_string(),
                }
            })
            .collect();

        let witnesses = ["wit-local", "wit-omen-01", "wit-fleet-2"];
        let verdicts = ["ATTESTED", "ATTESTED", "ATTESTED", "QUARANTINED"];
        let attestations: Vec<Attestation> = (0..40)
            .map(|i| Attestation {
                seq: 10_000 + i as u64,
                witness: witnesses[i % witnesses.len()].to_string(),
                memory_uuid: memories[i % memories.len()].uuid.clone(),
                verdict: verdicts[i % verdicts.len()].to_string(),
                ts_sec: now - (39 - i as u64) * 150,
            })
            .collect();

        // Prompt fingerprints (TYPE_PROMPT, content-addressed) linked to the
        // first memories — the UUID browser's prompt→memories source.
        let prompts: Vec<PromptRef> = (0..4)
            .map(|i| {
                let uuid = imbuuid_mem::prompt::fingerprint_prompt(
                    &format!("seeded browser probe prompt {}", i),
                    imbuuid_mem::gyst::PROV_CLI,
                );
                PromptRef {
                    uuid,
                    provenance: imbuuid_mem::gyst::PROV_CLI,
                    linked_memory_uuids: vec![memories[i % memories.len()].uuid.clone()],
                }
            })
            .collect();

        InMemoryStore {
            memories,
            attestations,
            dedup_collapses: 7,
            demo: true,
            prompts,
        }
    }

    /// Load a JSON snapshot: {"memories":[...],"attestations":[...],"dedup_collapses":n}.
    pub fn from_json_file(path: &str) -> Result<Self, String> {
        let data =
            std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path, e))?;
        serde_json::from_str::<InMemoryStore>(&data)
            .map(|mut s| {
                s.demo = false;
                s
            })
            .map_err(|e| format!("cannot parse {}: {}", path, e))
    }

    /// Simulate a live witness heartbeat: append one attestation, cap length.
    /// Documented simulation — the seam for the real witness stream.
    pub fn tick(&mut self) {
        let now = now_sec();
        let seq = self.attestations.last().map(|a| a.seq + 1).unwrap_or(10_000);
        let mem = self
            .memories
            .get((seq as usize) % self.memories.len().max(1))
            .map(|m| m.uuid.clone())
            .unwrap_or_default();
        self.attestations.push(Attestation {
            seq,
            witness: "wit-local".to_string(),
            memory_uuid: mem,
            verdict: "ATTESTED".to_string(),
            ts_sec: now,
        });
        if self.attestations.len() > 200 {
            let drop = self.attestations.len() - 200;
            self.attestations.drain(0..drop);
        }
    }
}

impl StoreSnapshot for InMemoryStore {
    fn stats(&self) -> StoreStats {
        let mut ns_map: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for m in &self.memories {
            *ns_map.entry(m.namespace.clone()).or_default() += 1;
        }
        let mut namespaces: Vec<(String, u64)> = ns_map.into_iter().collect();
        namespaces.sort_by(|a, b| b.1.cmp(&a.1));
        StoreStats {
            total_memories: self.memories.len() as u64,
            total_attestations: self.attestations.len() as u64,
            dedup_collapses: self.dedup_collapses,
            namespaces,
        }
    }

    fn attestations(&self) -> &[Attestation] {
        &self.attestations
    }

    fn memories(&self) -> &[MemoryEntry] {
        &self.memories
    }

    fn prompt_fingerprints(&self) -> Vec<&PromptRef> {
        self.prompts.iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_store_has_expected_shape() {
        let s = InMemoryStore::seeded();
        let stats = s.stats();
        assert_eq!(stats.total_memories, 12);
        assert_eq!(stats.total_attestations, 40);
        assert_eq!(stats.dedup_collapses, 7);
        assert_eq!(stats.namespaces.len(), 3);
        assert!(s.demo);
        // Feed is newest-first, capped.
        let feed = s.feed(5);
        assert_eq!(feed.len(), 5);
        assert!(feed[0].ts_sec >= feed[1].ts_sec);
        // Memory UUIDs are real TYPE_MEMORY v8 mints.
        for m in &s.memories {
            assert_eq!(m.uuid.len(), 36, "{}", m.uuid);
            assert_eq!(&m.uuid[14..15], "8", "version nibble: {}", m.uuid);
        }
    }

    #[test]
    fn tick_appends_and_caps() {
        let mut s = InMemoryStore::seeded();
        let before = s.attestations.len();
        s.tick();
        assert_eq!(s.attestations.len(), before + 1);
        assert_eq!(s.attestations.last().unwrap().verdict, "ATTESTED");
        for _ in 0..300 {
            s.tick();
        }
        assert_eq!(s.attestations.len(), 200);
    }

    #[test]
    fn json_snapshot_roundtrips() {
        let s = InMemoryStore::seeded();
        let json = serde_json::to_string(&s).unwrap();
        let dir = std::env::temp_dir();
        let path = dir.join("imbuuid-panel-test-snapshot.json");
        std::fs::write(&path, &json).unwrap();
        let loaded = InMemoryStore::from_json_file(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.memories.len(), 12);
        assert_eq!(loaded.attestations.len(), 40);
        assert!(!loaded.demo, "file-loaded snapshots are not demo");
        assert!(InMemoryStore::from_json_file("/nonexistent/path.json").is_err());
        let _ = std::fs::remove_file(&path);
    }
}
