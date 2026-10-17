// file_id: SOM-DOC-10433-v1.0.0 (part: memory)
// name: memory.rs — content-hash dedup memory store
//
// Canonical form:  format!("{}|{}", TYPE_MEMORY, normalized_content)
//   where normalized = trim + collapse whitespace runs.
// Content hash = low 42 bits of SHA-256(canonical), placed in the UUID's
// rand42 field. This is a DOCUMENTED codetable repurposing for memory types:
// the memory UUID is content-addressed — identical content (+namespace)
// mints the identical 42-bit identity slot, which is exactly what makes
// dedup structural rather than advisory.
//
// Encrypted mode (provenance forced to PROV_ENCRYPTED = 0xF):
//   hash = low 42 bits of HMAC-SHA256(hmac_key, canonical).
// Equality is only checkable with the key — two records minted under
// different keys are uncorrelated, and the plaintext hash cannot be
// derived from the encrypted one.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::gyst::{
    encode_gyst, fnv1a12, PROV_ENCRYPTED, TYPE_MEMORY,
};

/// Persistence snapshot file name inside the open directory.
pub const SNAPSHOT_FILE: &str = "memories.json";

/// Normalize: trim + collapse whitespace runs (split_whitespace join).
pub(crate) fn normalize(content: &str) -> String {
    content.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Canonical form hashed for dedup identity.
pub(crate) fn canonical_form(content: &str) -> String {
    format!("{}|{}", TYPE_MEMORY, normalize(content))
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before epoch")
        .as_secs()
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("hmac key required for encrypted insert")]
    MissingHmacKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub uuid: String,
    pub content: String,
    pub content_hash: u64,
    pub prompt_id: Option<String>,
    pub provenance: u8,
    pub namespace: u16,
    pub fractal_sector: u16,
    pub signal: f32,
    pub observation_count: u64,
    pub first_seen_sec: u64,
    pub last_seen_sec: u64,
}

#[derive(Debug, Clone)]
pub struct InsertOpts {
    pub namespace_label: String,
    pub prompt_id: Option<String>,
    pub provenance: u8,
    pub signal: f32,
    pub encrypted: bool,
    pub hmac_key: Option<[u8; 32]>,
}

impl InsertOpts {
    pub fn new(namespace_label: impl Into<String>) -> Self {
        Self {
            namespace_label: namespace_label.into(),
            prompt_id: None,
            provenance: crate::gyst::PROV_CLI,
            signal: 0.5,
            encrypted: false,
            hmac_key: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum InsertOutcome {
    Created(MemoryRecord),
    Duplicate { uuid: String, observation_count: u64 },
}

/// In-memory HashMap store with whole-snapshot JSON persistence (v0).
///
/// Snapshot layout at `<dir>/memories.json` (v0, documented): a single JSON
/// object `{ "version": 0, "memories": [...] }` holding every record. Loaded
/// on `open`, rewritten on `flush` (and on every `insert` when a directory
/// is set). v0 is deliberately whole-snapshot; delta streams are future work.
pub struct MemoryStore {
    records: HashMap<String, MemoryRecord>,
    /// (content_hash, namespace) -> uuid dedup index.
    index: HashMap<(u64, u16), String>,
    dir: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    memories: Vec<MemoryRecord>,
}

impl MemoryStore {
    /// Open a store. `None` = in-memory only (no persistence).
    /// `Some(dir)` = load `memories.json` if present; `flush` writes it.
    pub fn open(path: Option<PathBuf>) -> Result<Self, StoreError> {
        let mut store = Self {
            records: HashMap::new(),
            index: HashMap::new(),
            dir: path,
        };
        if let Some(dir) = &store.dir {
            let snap_path = dir.join(SNAPSHOT_FILE);
            if snap_path.exists() {
                let data = std::fs::read(&snap_path)?;
                let snap: Snapshot = serde_json::from_slice(&data)?;
                for rec in snap.memories {
                    store
                        .index
                        .insert((rec.content_hash, rec.namespace), rec.uuid.clone());
                    store.records.insert(rec.uuid.clone(), rec);
                }
                tracing::info!(
                    dir = %dir.display(),
                    count = store.records.len(),
                    "memory.open.loaded"
                );
            }
        }
        Ok(store)
    }

    /// Insert content. Same (content_hash, namespace) -> `Duplicate`:
    /// observation_count incremented, last_seen=now, no new record.
    pub fn insert(&mut self, content: &str, opts: InsertOpts) -> Result<InsertOutcome, StoreError> {
        let norm = normalize(content);
        let canonical = canonical_form(content);
        let content_hash = if opts.encrypted {
            let key = opts.hmac_key.ok_or(StoreError::MissingHmacKey)?;
            let mut mac = Hmac::<Sha256>::new_from_slice(&key)
                .expect("HMAC accepts any key length");
            mac.update(canonical.as_bytes());
            let out = mac.finalize().into_bytes();
            let mut r: u64 = 0;
            for b in out.iter().skip(24) {
                r = (r << 8) | (*b as u64);
            }
            r & ((1u64 << 42) - 1)
        } else {
            crate::sha256_low42(canonical.as_bytes())
        };
        let namespace = fnv1a12(&opts.namespace_label);
        let now = now_sec();

        if let Some(uuid) = self.index.get(&(content_hash, namespace)) {
            let rec = self
                .records
                .get_mut(uuid)
                .expect("dedup index and records out of sync");
            rec.observation_count += 1;
            rec.last_seen_sec = now;
            let outcome = InsertOutcome::Duplicate {
                uuid: uuid.clone(),
                observation_count: rec.observation_count,
            };
            tracing::info!(
                uuid = %uuid,
                observation_count = rec.observation_count,
                "memory.insert.duplicate"
            );
            self.auto_flush()?;
            return Ok(outcome);
        }

        let provenance = if opts.encrypted {
            PROV_ENCRYPTED
        } else {
            opts.provenance
        };
        let uuid = encode_gyst(
            TYPE_MEMORY,
            namespace,
            now,
            0, // fractal depth
            0, // fractal domain
            0, // fractal generation
            opts.signal,
            provenance,
            content_hash, // content-addressed rand42 (documented repurposing)
        );
        let rec = MemoryRecord {
            uuid: uuid.clone(),
            content: content.to_string(),
            content_hash,
            prompt_id: opts.prompt_id.clone(),
            provenance,
            namespace,
            fractal_sector: fnv1a12(&norm),
            signal: opts.signal,
            observation_count: 1,
            first_seen_sec: now,
            last_seen_sec: now,
        };
        self.index.insert((content_hash, namespace), uuid.clone());
        self.records.insert(uuid.clone(), rec.clone());
        tracing::info!(uuid = %uuid, provenance = provenance, "memory.insert.created");
        self.auto_flush()?;
        Ok(InsertOutcome::Created(rec))
    }

    fn auto_flush(&self) -> Result<(), StoreError> {
        if self.dir.is_some() {
            self.flush()?;
        }
        Ok(())
    }

    /// Write the whole-snapshot v0 file (no-op for in-memory stores).
    pub fn flush(&self) -> Result<(), StoreError> {
        if let Some(dir) = &self.dir {
            std::fs::create_dir_all(dir)?;
            let snap = Snapshot {
                version: 0,
                memories: self.records.values().cloned().collect(),
            };
            let data = serde_json::to_string_pretty(&snap)?;
            std::fs::write(dir.join(SNAPSHOT_FILE), data)?;
            tracing::info!(
                dir = %dir.display(),
                count = self.records.len(),
                "memory.flush"
            );
        }
        Ok(())
    }

    pub fn get(&self, uuid: &str) -> Option<&MemoryRecord> {
        self.records.get(uuid)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &MemoryRecord> {
        self.records.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gyst::{decode_gyst, PROV_CLI};

    fn opts(ns: &str) -> InsertOpts {
        InsertOpts {
            namespace_label: ns.to_string(),
            prompt_id: None,
            provenance: PROV_CLI,
            signal: 0.5,
            encrypted: false,
            hmac_key: None,
        }
    }

    #[test]
    fn dedup_same_content_twice() {
        let mut s = MemoryStore::open(None).unwrap();
        let r1 = s.insert("hello world", opts("t")).unwrap();
        let uuid = match r1 {
            InsertOutcome::Created(rec) => rec.uuid,
            _ => panic!("expected Created"),
        };
        let r2 = s.insert("hello world", opts("t")).unwrap();
        match r2 {
            InsertOutcome::Duplicate {
                uuid: u2,
                observation_count,
            } => {
                assert_eq!(u2, uuid);
                assert_eq!(observation_count, 2);
            }
            _ => panic!("expected Duplicate"),
        }
        assert_eq!(s.len(), 1);
        assert_eq!(s.get(&uuid).unwrap().observation_count, 2);
    }

    #[test]
    fn dedup_normalizes_whitespace() {
        let mut s = MemoryStore::open(None).unwrap();
        s.insert("hello world", opts("t")).unwrap();
        let r = s.insert("  hello \t  world\n", opts("t")).unwrap();
        assert!(matches!(r, InsertOutcome::Duplicate { observation_count: 2, .. }));
    }

    #[test]
    fn different_content_distinct_hashes() {
        let mut s = MemoryStore::open(None).unwrap();
        let a = s.insert("alpha", opts("t")).unwrap();
        let b = s.insert("beta", opts("t")).unwrap();
        let (ua, ub) = match (a, b) {
            (InsertOutcome::Created(a), InsertOutcome::Created(b)) => (a.uuid, b.uuid),
            _ => panic!(),
        };
        assert_ne!(ua, ub);
        assert_ne!(
            s.get(&ua).unwrap().content_hash,
            s.get(&ub).unwrap().content_hash
        );
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn same_content_different_namespaces_are_distinct() {
        let mut s = MemoryStore::open(None).unwrap();
        s.insert("shared", opts("a")).unwrap();
        s.insert("shared", opts("b")).unwrap();
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn encrypted_requires_key_and_is_uncorrelated() {
        let mut s = MemoryStore::open(None).unwrap();
        // No key -> error.
        let bad = InsertOpts {
            encrypted: true,
            ..opts("t")
        };
        assert!(matches!(
            s.insert("secret", bad).unwrap_err(),
            StoreError::MissingHmacKey
        ));
        // Same content, different keys -> different hashes (uncorrelated).
        let k1 = [1u8; 32];
        let k2 = [2u8; 32];
        let o1 = InsertOpts {
            encrypted: true,
            hmac_key: Some(k1),
            ..opts("t")
        };
        let o2 = InsertOpts {
            encrypted: true,
            hmac_key: Some(k2),
            ..opts("t")
        };
        let h1 = match s.insert("secret", o1).unwrap() {
            InsertOutcome::Created(r) => r.content_hash,
            _ => panic!(),
        };
        let h2 = match s.insert("secret", o2).unwrap() {
            InsertOutcome::Created(r) => r.content_hash,
            _ => panic!(),
        };
        assert_ne!(h1, h2);
        // Without the key you cannot correlate: encrypted hash != plaintext hash.
        let plain = match s.insert("secret", opts("t")).unwrap() {
            InsertOutcome::Created(r) => r.content_hash,
            _ => panic!(),
        };
        assert_ne!(h1, plain);
        assert_ne!(h2, plain);
        // Provenance forced to PROV_ENCRYPTED in the UUID.
        let enc_uuid = s
            .iter()
            .find(|r| r.content_hash == h1)
            .unwrap()
            .uuid
            .clone();
        assert_eq!(decode_gyst(&enc_uuid).unwrap().provenance, PROV_ENCRYPTED);
    }

    #[test]
    fn memory_uuid_is_type_memory_v8() {
        let mut s = MemoryStore::open(None).unwrap();
        let uuid = match s.insert("typed", opts("t")).unwrap() {
            InsertOutcome::Created(r) => r.uuid,
            _ => panic!(),
        };
        let d = decode_gyst(&uuid).unwrap();
        assert_eq!(d.type_code, TYPE_MEMORY);
        assert_eq!(d.version, 8);
        assert_eq!(d.variant, 2);
    }
}
