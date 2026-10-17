// file_id: SOM-DOC-10434 (part)
// name: src/demo.rs
// description: `--demo` seed corpus — 25 sample memories across 2 namespaces
//   ("ops", "research") with varied signals, observation counts, timestamps,
//   prompt links and witness attestations, for first-run feel.
// project_id: IMBUUID-MEM
// category: tui
// created: 2026-10-17
// version: 1.0.0

/// One demo memory. `*_days_ago` are relative to seed time; the harness store
/// stamps first_seen/last_seen at insert, so these are informational weight.
pub struct DemoSample {
    pub content: &'static str,
    pub namespace: &'static str,
    pub signal: f32,
    pub prompt: Option<&'static str>,
    /// Whether this sample gets a witness attestation at seed time.
    pub attest: bool,
}

pub fn samples() -> Vec<DemoSample> {
    vec![
        // ---- ops (13) ----
        DemoSample { content: "protonsb deploy checklist: hitachi gateway firmware 4.2.1 staged on all 3 nodes; rollback image pinned before cutover", namespace: "ops", signal: 0.92, prompt: Some("summarize today's deploy state"), attest: true },
        DemoSample { content: "bakar bldg B MEP rough-in 68% — LV tray conflicts with med-gas at level 3 east; RFI-114 open with GC", namespace: "ops", signal: 0.88, prompt: None, attest: true },
        DemoSample { content: "smartsheet webhook ledger: replayed 214 events after outage, 0 dupes applied — idempotency key held", namespace: "ops", signal: 0.81, prompt: Some("replay the webhook ledger"), attest: true },
        DemoSample { content: "on-call rotation: paged 02:14 for imbuy-api latency p99 > 800ms; traced to vercel cold start + missing index", namespace: "ops", signal: 0.74, prompt: None, attest: true },
        DemoSample { content: "raid log #47: risk — proton vendor docs still not delivered; mitigation: model from open standards only", namespace: "ops", signal: 0.95, prompt: Some("what are the top risks?"), attest: true },
        DemoSample { content: "commissioning script v9 passed on node 2; node 3 fails at step 41 (interlock timing) — vendor ticket open", namespace: "ops", signal: 0.69, prompt: None, attest: false },
        DemoSample { content: "milestone M4 baseline moved +6d after OSHPD submittal review; critical path now through LV rough-in", namespace: "ops", signal: 0.83, prompt: None, attest: true },
        DemoSample { content: "gumroad payout reconciled: $1,240.00 for october tranche; 2 refunds (duplicate purchases) processed", namespace: "ops", signal: 0.55, prompt: None, attest: false },
        DemoSample { content: "omen-01 fleet heartbeat: herdr agents 4/4 green, mailbox poll latency 1.2s p50 — no action needed", namespace: "ops", signal: 0.41, prompt: None, attest: false },
        DemoSample { content: "supabase keep-alive cron verified: db survived 7-day inactivity window, no pause this cycle", namespace: "ops", signal: 0.62, prompt: None, attest: true },
        DemoSample { content: "installer signing: signpath OSS cert renewed; azure trusted signing still the smartscreen fix — not yet purchased", namespace: "ops", signal: 0.58, prompt: None, attest: false },
        DemoSample { content: "itpm powerstation ingest: 5 api-shaped sheets defined (requirements, raid, commissioning, milestones, ledger)", namespace: "ops", signal: 0.87, prompt: Some("define the powerstation sheets"), attest: true },
        DemoSample { content: "devin max plan meter: $200 active until nov 9; session 1 (supabase durable storage) still unbuilt", namespace: "ops", signal: 0.77, prompt: None, attest: true },
        // ---- research (12) ----
        DemoSample { content: "harvest: mem0 ADD-only extraction converges with uuid-chain append-only doctrine; extraction LLM cost violates spending law if naive", namespace: "research", signal: 0.9, prompt: Some("evaluate mem0 for imbue-memory"), attest: true },
        DemoSample { content: "harvest: digup multimodal embedding index (768-dim) — moment-level addressing (video@ts, pdf@page) is the keeper capability", namespace: "research", signal: 0.86, prompt: None, attest: true },
        DemoSample { content: "harvest: rustgis layered crates L0-L6 — native 3d globe answers the scpm verdict's hardest part; pin geom/data/map/tiles/render", namespace: "research", signal: 0.84, prompt: None, attest: true },
        DemoSample { content: "harvest: openviking AGPL-3.0 — hard license boundary, ideas only; filesystem-addressable memory validates our direction", namespace: "research", signal: 0.79, prompt: None, attest: false },
        DemoSample { content: "harvest: hindsight retain/recall/reflect triad — reflect (disposition-aware generation) is what mem0 lacks", namespace: "research", signal: 0.82, prompt: Some("compare hindsight vs mem0"), attest: true },
        DemoSample { content: "uuid language wave: chain+walker proven 12/12 tests; red-team M1d fix PROVEN; 30 artifacts chained, full walk CHAIN OK", namespace: "research", signal: 0.93, prompt: None, attest: true },
        DemoSample { content: "qr credential design: mint 0xa77 provisional -> attenuate -> hmac-sign -> qr; static qr is bearer credential, issuer compromise = total", namespace: "research", signal: 0.71, prompt: None, attest: false },
        DemoSample { content: "mycelial dossier protocol: mycelium proposes, jev decides, ssfde dithering spreads picks — receipts hash-chained", namespace: "research", signal: 0.76, prompt: Some("explain the dossier protocol"), attest: true },
        DemoSample { content: "tokenage: dormant authorized api capacity as metered commodity; hard line — only capacity authorized for resale, harness enforces tos", namespace: "research", signal: 0.73, prompt: None, attest: true },
        DemoSample { content: "geohex anchor law: position from versioned geodetic anchor outside uuid; SF_ANCHOR_V0 candidate 37.785,-122.4 not ratified — no minting", namespace: "research", signal: 0.89, prompt: None, attest: true },
        DemoSample { content: "says.com layer 11: uuid namespace registry replaces dns lookups; honest scope — uuids replace naming, not packet routing (bgp still moves bytes)", namespace: "research", signal: 0.66, prompt: None, attest: false },
        DemoSample { content: "capability router: staged retrieval -> ~10 shortlist -> jev typed-choice -> model-tie shrink-only binding, all receipted", namespace: "research", signal: 0.8, prompt: Some("how does capability selection work?"), attest: true },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn demo_has_25_samples_across_exactly_2_namespaces() {
        let s = samples();
        assert_eq!(s.len(), 25);
        let ns: HashSet<&str> = s.iter().map(|x| x.namespace).collect();
        assert_eq!(ns.len(), 2);
        assert!(ns.contains("ops"));
        assert!(ns.contains("research"));
    }

    #[test]
    fn demo_contents_are_nonempty_and_signals_in_range() {
        for s in samples() {
            assert!(!s.content.is_empty());
            assert!((0.0..=1.0).contains(&s.signal), "signal out of range: {}", s.signal);
        }
    }
}
