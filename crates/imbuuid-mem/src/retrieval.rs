// file_id: SOM-DOC-10433-v1.0.0 (part: retrieval)
// name: retrieval.rs — four-stage retrieval pipeline
//
// Stage 1 — fractal(12) sector pre-filter:
//   qsector = fnv1a12(normalized query); keep iff
//   (mem.fractal_sector ^ qsector).count_ones() <= sector_tolerance_bits
//   (default 2). This is a coarse spatial-sector lens over the 4096-sector
//   field — it groups, it never leaf-addresses.
// Stage 2 — sig(16) comparison:
//   sig_score = 1.0 - |mem.signal - qsignal|, where qsignal comes from
//   opts.signal, default 0.5 neutral (v0 — documented; a real query-signal
//   estimator is future work).
// Stage 3 — dedup-collapse on content_hash (defensive): same content that
//   entered under different namespaces collapses to the single
//   highest-scored record.
// Stage 4 — frequency-weighted ranking:
//   score = 0.5*sector_score + 0.3*sig_score + 0.2*freq_score
//   sector_score = 1 - hamming/12
//   freq_score   = ln(1+count) / ln(1+max_count_in_candidates)
//   (re-observation is evidence: frequently re-observed memories rank higher,
//   ceteris paribus). Results sorted descending, truncated to top_k.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::gyst::fnv1a12;
use crate::memory::{normalize, MemoryRecord, MemoryStore};

pub struct QueryOpts {
    pub namespace_label: Option<String>,
    pub top_k: usize,
    pub sector_tolerance_bits: u32,
    pub signal: Option<f32>,
}

impl QueryOpts {
    /// Defaults: no namespace filter, top_k 10, sector tolerance 2 bits,
    /// neutral query signal 0.5.
    pub fn new() -> Self {
        Self {
            namespace_label: None,
            top_k: 10,
            sector_tolerance_bits: 2,
            signal: None,
        }
    }
}

impl Default for QueryOpts {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    pub uuid: String,
    pub score: f32,
    pub observation_count: u64,
}

pub fn retrieve(store: &MemoryStore, query: &str, opts: &QueryOpts) -> Vec<ScoredMemory> {
    let span = tracing::info_span!("retrieval.retrieve", top_k = opts.top_k);
    let _guard = span.enter();

    let qsector = fnv1a12(&normalize(query));
    let qsignal = opts.signal.unwrap_or(0.5); // v0 neutral default
    let ns_filter = opts.namespace_label.as_deref().map(fnv1a12);

    // ---- Stage 1+2: sector pre-filter, namespace filter, signal score ----
    let mut prelim: Vec<(&MemoryRecord, f32, f32)> = Vec::new();
    for mem in store.iter() {
        if let Some(ns) = ns_filter {
            if mem.namespace != ns {
                continue;
            }
        }
        let hamming = (mem.fractal_sector ^ qsector).count_ones();
        if hamming > opts.sector_tolerance_bits {
            continue;
        }
        let sector_score = 1.0 - hamming as f32 / 12.0;
        let sig_score = 1.0 - (mem.signal - qsignal).abs();
        prelim.push((mem, sector_score, sig_score));
    }

    // ---- Stage 3: dedup-collapse on content_hash, keep higher pre-score ----
    // pre_score = 0.5*sector + 0.3*sig (the freq term is stage 4).
    let mut best: HashMap<u64, (&MemoryRecord, f32, f32, f32)> = HashMap::new();
    for (mem, sector_score, sig_score) in prelim {
        let pre = 0.5 * sector_score + 0.3 * sig_score;
        match best.get(&mem.content_hash) {
            Some((_, _, _, kept_pre)) if *kept_pre >= pre => {}
            _ => {
                best.insert(mem.content_hash, (mem, sector_score, sig_score, pre));
            }
        }
    }

    // ---- Stage 4: frequency-weighted ranking ----
    let max_count = best
        .values()
        .map(|(m, _, _, _)| m.observation_count)
        .max()
        .unwrap_or(1);
    let denom = (1.0 + max_count as f32).ln();

    let mut scored: Vec<ScoredMemory> = best
        .into_values()
        .map(|(mem, sector_score, sig_score, _)| {
            let freq_score = (1.0 + mem.observation_count as f32).ln() / denom;
            let score = 0.5 * sector_score + 0.3 * sig_score + 0.2 * freq_score;
            ScoredMemory {
                uuid: mem.uuid.clone(),
                score,
                observation_count: mem.observation_count,
            }
        })
        .collect();
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(opts.top_k);

    tracing::info!(
        query_len = query.len(),
        returned = scored.len(),
        "retrieval.retrieve"
    );
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gyst::{decode_gyst, PROV_CLI};
    use crate::memory::{InsertOpts, MemoryStore};

    fn opts(ns: &str, signal: f32) -> InsertOpts {
        InsertOpts {
            namespace_label: ns.to_string(),
            prompt_id: None,
            provenance: PROV_CLI,
            signal,
            encrypted: false,
            hmac_key: None,
        }
    }

    /// Find two distinct contents sharing the same 12-bit sector (pigeonhole:
    /// >4096 probes guarantee a collision; deterministic, no RNG).
    fn colliding_pair() -> (String, String) {
        let mut seen: HashMap<u16, String> = HashMap::new();
        for i in 0..20000 {
            let c = format!("retrieval-probe-{:05}", i);
            let h = fnv1a12(&c);
            if let Some(prev) = seen.insert(h, c.clone()) {
                return (prev, c);
            }
        }
        panic!("no collision in 20000 probes (impossible)")
    }

    #[test]
    fn sector_prefilter_narrows_and_ranking_prefers_frequency() {
        let (pa, pb) = colliding_pair();
        let mut store = MemoryStore::open(None).unwrap();

        // A: count 1. B: same sector, same signal, count 3 (re-observed).
        store.insert(&pa, opts("alpha", 0.5)).unwrap();
        for _ in 0..3 {
            store.insert(&pb, opts("alpha", 0.5)).unwrap();
        }
        // A-content again under a second namespace: same content_hash,
        // must collapse to one result in retrieval.
        store.insert(&pa, opts("beta", 0.5)).unwrap();
        // 18 more across two namespaces, varied signals.
        for i in 0..18 {
            let ns = if i % 2 == 0 { "alpha" } else { "beta" };
            let sig = 0.07 + 0.043 * (i as f32);
            store
                .insert(&format!("background memory item {:02}", i), opts(ns, sig))
                .unwrap();
        }
        assert_eq!(store.len(), 21); // 20 unique + 1 beta-dup of A

        // Query = A's text: qsector == sector(A) == sector(B), hamming 0.
        let mut q = QueryOpts::new();
        q.top_k = 100;
        let narrow = retrieve(&store, &pa, &q);
        let mut qwide = QueryOpts::new();
        qwide.top_k = 100;
        qwide.sector_tolerance_bits = 12;
        let wide = retrieve(&store, &pa, &qwide);

        // Prefilter narrows: tol-2 set is a strict subset of the tol-12 set.
        assert_eq!(wide.len(), 20); // 21 records, A collapsed
        assert!(narrow.len() >= 2); // A and B both pass (hamming 0)
        assert!(narrow.len() < wide.len());

        // Every returned record satisfies the sector predicate (invariant).
        let qsector = fnv1a12(&normalize(&pa));
        for r in &narrow {
            let rec = store.get(&r.uuid).unwrap();
            assert!((rec.fractal_sector ^ qsector).count_ones() <= 2);
        }

        // Dedup-collapse: exactly one result per content_hash in the wide set.
        let hash_a = decode_gyst(&store.iter().find(|r| r.content == pa && r.namespace == fnv1a12("alpha")).unwrap().uuid)
            .unwrap()
            .random;
        let a_results: Vec<_> = wide
            .iter()
            .filter(|r| decode_gyst(&r.uuid).unwrap().random == hash_a)
            .collect();
        assert_eq!(a_results.len(), 1);

        // Ceteris paribus: B (count 3) ranks strictly before A (count 1) —
        // identical sector_score and sig_score, only freq differs.
        let hash_b = decode_gyst(
            &store
                .iter()
                .find(|r| r.content == pb)
                .unwrap()
                .uuid,
        )
        .unwrap()
        .random;
        let pos_b = narrow.iter().position(|r| decode_gyst(&r.uuid).unwrap().random == hash_b).unwrap();
        let pos_a = narrow.iter().position(|r| decode_gyst(&r.uuid).unwrap().random == hash_a).unwrap();
        assert!(pos_b < pos_a, "B(count=3) must rank before A(count=1)");
        // And B attains the global maximum score 1.0.
        assert!((narrow[pos_b].score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn namespace_filter_and_signal_default() {
        let mut store = MemoryStore::open(None).unwrap();
        store.insert("only alpha", opts("alpha", 0.9)).unwrap();
        store.insert("only beta", opts("beta", 0.1)).unwrap();
        let mut q = QueryOpts::new();
        q.sector_tolerance_bits = 12; // no-op prefilter
        q.top_k = 100;
        q.namespace_label = Some("alpha".to_string());
        let r = retrieve(&store, "only", &q);
        assert_eq!(r.len(), 1);
        assert_eq!(store.get(&r[0].uuid).unwrap().namespace, fnv1a12("alpha"));
    }

    #[test]
    fn defaults_documented() {
        let q = QueryOpts::new();
        assert_eq!(q.sector_tolerance_bits, 2);
        assert_eq!(q.top_k, 10);
        assert!(q.signal.is_none()); // -> 0.5 neutral at retrieve time
        assert!(q.namespace_label.is_none());
    }
}
