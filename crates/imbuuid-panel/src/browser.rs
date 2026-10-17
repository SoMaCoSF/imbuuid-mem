// SOM-DOC-10435 (part) — UUID browser logic: source → decode → resolve → render.
//
// Doctrine: the UUID is the address; the browser decodes it with
// imbuuid-mem's gyst codec and resolves whatever the local store holds at
// that address. A decodable UUID with no local record renders as an honest
// miss — record contents are never invented.

use crate::store::StoreSnapshot;
use chrono::{TimeZone, Utc};
use imbuuid_mem::gyst::{
    decode_gyst, PROV_AGENT, PROV_CLAUDE, PROV_CLI, PROV_DASHBOARD, PROV_DEXTER, PROV_ENCRYPTED,
    PROV_POLY_MAKER, PROV_REGISTRY, PROV_TURBO_QUANT, PROV_UNKNOWN, TYPE_ATTESTATION,
    TYPE_BINARY_PROVENANCE, TYPE_MEMORY, TYPE_POLY_MARKET, TYPE_POLY_OUTCOME_QUOTE, TYPE_PROMPT,
    TYPE_TURBO_ATTN, TYPE_TURBO_KV, TYPE_TURBO_LAYER,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::time::{SystemTime, UNIX_EPOCH};

const GOLD: Color = Color::Rgb(201, 162, 39);
const DIM: Color = Color::Rgb(120, 130, 145);
const CYAN: Color = Color::Rgb(86, 182, 194);
const RED: Color = Color::Rgb(200, 70, 70);
const GREEN: Color = Color::Rgb(110, 180, 110);

/// The UUIDv8 scan pattern (same contract as the imbuuid-rea extractor):
/// version nibble pinned to `8`, RFC 9562 variant bits `[89ab]`, hex body.
/// Documented here as the contract; `extract_uuids` implements it without
/// a regex dependency.
#[allow(dead_code)]
pub const UUIDV8_PATTERN: &str =
    "[0-9a-f]{8}-[0-9a-f]{4}-8[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";

/// The dossier→browser flow: dossiers carry UUIDs, the browser walks them.
/// Byte-window scan implementing UUIDV8_PATTERN (no regex dep needed).
pub fn extract_uuids(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut out: Vec<String> = Vec::new();
    if b.len() < 36 {
        return out;
    }
    for i in 0..=b.len() - 36 {
        let w = &b[i..i + 36];
        let template_ok = w[8] == b'-' && w[13] == b'-' && w[18] == b'-' && w[23] == b'-';
        let version_ok = w[14] == b'8';
        let variant_ok = matches!(w[19], b'8' | b'9' | b'a' | b'b' | b'A' | b'B');
        let hex_ok = w
            .iter()
            .enumerate()
            .all(|(j, c)| matches!(j, 8 | 13 | 18 | 23) || c.is_ascii_hexdigit());
        if template_ok && version_ok && variant_ok && hex_ok {
            let s = String::from_utf8_lossy(w).to_lowercase();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

/// Read a dossier `.md` and extract exactly the UUIDs it carries.
pub fn load_dossier_uuids(path: &str) -> Result<Vec<String>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path, e))?;
    Ok(extract_uuids(&text))
}

/// Contract-known type names (imbuuid-mem codetable, provisional 0x400-0x403).
pub fn type_name(code: u16) -> String {
    match code {
        TYPE_MEMORY => "MEMORY".to_string(),
        TYPE_PROMPT => "PROMPT".to_string(),
        TYPE_ATTESTATION => "ATTESTATION".to_string(),
        TYPE_BINARY_PROVENANCE => "BINARY_PROVENANCE".to_string(),
        TYPE_TURBO_KV => "TURBO_KV".to_string(),
        TYPE_TURBO_ATTN => "TURBO_ATTN".to_string(),
        TYPE_TURBO_LAYER => "TURBO_LAYER".to_string(),
        TYPE_POLY_MARKET => "POLY_MARKET".to_string(),
        TYPE_POLY_OUTCOME_QUOTE => "POLY_OUTCOME_QUOTE".to_string(),
        _ => format!("UNKNOWN(0x{:03x})", code),
    }
}

/// Contract-known provenance names.
pub fn prov_name(prov: u8) -> &'static str {
    match prov {
        PROV_UNKNOWN => "UNKNOWN",
        PROV_DEXTER => "DEXTER",
        PROV_CLI => "CLI",
        PROV_CLAUDE => "CLAUDE",
        PROV_DASHBOARD => "DASHBOARD",
        PROV_REGISTRY => "REGISTRY",
        PROV_AGENT => "AGENT",
        PROV_POLY_MAKER => "POLY_MAKER",
        PROV_TURBO_QUANT => "TURBO_QUANT",
        PROV_ENCRYPTED => "ENCRYPTED",
        _ => "UNKNOWN",
    }
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The 24-bit timestamp field wraps every 2^24 s (~194 days). Reconstruct
/// the nearest window to `now_sec` — same convention as the REA extractor.
pub fn resolve_ts24(ts24: u32, now: u64) -> u64 {
    const WRAP: u64 = 1 << 24;
    let t = ts24 as u64;
    let base = now.saturating_sub(now % WRAP);
    let mut cand = base + t;
    if cand > now + WRAP / 2 {
        cand -= WRAP;
    }
    cand
}

pub fn fmt_utc(sec: u64) -> String {
    Utc.timestamp_opt(sec as i64, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// The decoded address, rendered. Timestamp is labelled as a ts24
/// window-fit so the ~194-day wrap is never hidden.
#[derive(Debug, Clone)]
pub struct DecodedView {
    pub uuid: String,
    pub type_code: u16,
    pub type_label: String,
    pub namespace: u16,
    pub ts24: u32,
    pub ts_utc: String,
    pub provenance: u8,
    pub provenance_label: String,
    pub signal: f32,
    pub fractal_depth: u8,
    pub fractal_domain: u8,
    pub fractal_generation: u8,
    pub fractal_sector: u16,
}

/// Decode a candidate UUID string. Rejects non-v8 input honestly —
/// decode_gyst accepts any 32-hex, so version/variant are checked here.
pub fn decode_view(input: &str) -> Result<DecodedView, String> {
    let d = decode_gyst(input).map_err(|e| e.to_string())?;
    if d.version != 8 || d.variant != 2 {
        return Err(format!(
            "not a UUIDv8 (version={} variant={}); the browser only walks v8 addresses",
            d.version, d.variant
        ));
    }
    let sector = ((d.fractal_depth as u16) << 8)
        | ((d.fractal_domain as u16) << 4)
        | (d.fractal_generation as u16);
    let ts = resolve_ts24(d.timestamp_sec, now_sec());
    Ok(DecodedView {
        uuid: input.trim().to_lowercase(),
        type_code: d.type_code,
        type_label: type_name(d.type_code),
        namespace: d.namespace,
        ts24: d.timestamp_sec,
        ts_utc: format!("{} (ts24 window-fit)", fmt_utc(ts)),
        provenance: d.provenance,
        provenance_label: prov_name(d.provenance).to_string(),
        signal: d.signal_normalized,
        fractal_depth: d.fractal_depth,
        fractal_domain: d.fractal_domain,
        fractal_generation: d.fractal_generation,
        fractal_sector: sector,
    })
}

/// What the store holds at a decoded address. HonestMiss is a first-class
/// outcome: the UUID decoded fine, but nothing is stored at it here.
#[derive(Debug, Clone)]
pub enum Resolution {
    Memory {
        view: DecodedView,
        namespace_label: String,
        signal: f64,
        content: String,
        content_hash: String,
        first_seen: String,
        attestation_count: u32,
        receipt_lines: Vec<String>,
        prompt_links: Vec<String>,
    },
    Prompt {
        view: DecodedView,
        linked_memories: Vec<(String, String)>, // (uuid, namespace label)
    },
    Attestation {
        view: DecodedView,
        feed_lines: Vec<String>,
    },
    HonestMiss {
        view: DecodedView,
    },
    DecodeError {
        input: String,
        error: String,
    },
}

/// Decode → resolve. Never invents record contents.
pub fn resolve(input: &str, store: &dyn StoreSnapshot) -> Resolution {
    let view = match decode_view(input) {
        Ok(v) => v,
        Err(e) => {
            tracing::info!(input = %input, error = %e, "browser.decode_error");
            return Resolution::DecodeError {
                input: input.trim().to_string(),
                error: e,
            };
        }
    };
    let uuid = view.uuid.clone();
    let res = match view.type_code {
        TYPE_MEMORY => match store.lookup_memory(&uuid) {
            Some(m) => {
                let atts = store.attestations_for(&uuid);
                let receipts = atts
                    .iter()
                    .map(|a| {
                        format!(
                            "#{} {} {} {}",
                            a.seq,
                            a.witness,
                            a.verdict,
                            fmt_utc(a.ts_sec)
                        )
                    })
                    .collect();
                let prompts: Vec<String> = store
                    .prompt_fingerprints()
                    .iter()
                    .filter(|p| p.linked_memory_uuids.iter().any(|u| u == &uuid))
                    .map(|p| p.uuid.clone())
                    .collect();
                Resolution::Memory {
                    view,
                    namespace_label: m.namespace.clone(),
                    signal: m.signal,
                    content: m.summary.clone(),
                    content_hash: m.content_hash.clone(),
                    // The panel snapshot plane carries no per-record observation
                    // count (that's the live store's MemoryRecord); the witness
                    // feed below is the receipt evidence.
                    first_seen: fmt_utc(m.created_sec),
                    attestation_count: m.attestations,
                    receipt_lines: receipts,
                    prompt_links: prompts,
                }
            }
            None => Resolution::HonestMiss { view },
        },
        TYPE_PROMPT => {
            let linked_uuids: Vec<String> = store
                .prompt_links(&uuid)
                .into_iter()
                .map(|s| s.to_string())
                .collect();
            if linked_uuids.is_empty() {
                Resolution::HonestMiss { view }
            } else {
                let linked: Vec<(String, String)> = linked_uuids
                    .iter()
                    .filter_map(|u| store.lookup_memory(u))
                    .map(|m| (m.uuid.clone(), m.namespace.clone()))
                    .collect();
                Resolution::Prompt {
                    view,
                    linked_memories: linked,
                }
            }
        }
        TYPE_ATTESTATION => {
            let hits = store.attestations_for(&uuid);
            if hits.is_empty() {
                Resolution::HonestMiss { view }
            } else {
                let feed_lines = hits
                    .iter()
                    .map(|a| {
                        format!(
                            "#{} {} → {:.8}… {} {}",
                            a.seq,
                            a.witness,
                            a.memory_uuid,
                            a.verdict,
                            fmt_utc(a.ts_sec)
                        )
                    })
                    .collect();
                Resolution::Attestation { view, feed_lines }
            }
        }
        _ => Resolution::HonestMiss { view },
    };
    tracing::info!(uuid = %uuid, kind = ?std::mem::discriminant(&res), "browser.resolve");
    res
}

/// Every UUID the store source can walk: memory records + prompt fingerprints.
pub fn store_uuids(store: &dyn StoreSnapshot) -> Vec<String> {
    let mut out: Vec<String> = store.memories().iter().map(|m| m.uuid.clone()).collect();
    for p in store.prompt_fingerprints() {
        if !out.contains(&p.uuid) {
            out.push(p.uuid.clone());
        }
    }
    out
}

fn kv<'a>(k: &str, v: String, c: Color) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{:<14}", k), Style::default().fg(DIM)),
        Span::styled(v, Style::default().fg(c)),
    ])
}

fn decoded_lines(view: &DecodedView) -> Vec<Line<'static>> {
    vec![
        kv("uuid", view.uuid.clone(), Color::White),
        kv("type", format!("{} (0x{:03x})", view.type_label, view.type_code), GOLD),
        kv("namespace", format!("0x{:03x}", view.namespace), CYAN),
        kv("timestamp", view.ts_utc.clone(), Color::White),
        kv("ts24 raw", format!("0x{:06x}", view.ts24), DIM),
        kv("provenance", format!("{} (0x{:x})", view.provenance_label, view.provenance), CYAN),
        kv("signal", format!("{:.4}", view.signal), GREEN),
        kv(
            "fractal",
            format!(
                "sector 0x{:03x} (depth {} · domain {} · gen {})",
                view.fractal_sector, view.fractal_depth, view.fractal_domain, view.fractal_generation
            ),
            DIM,
        ),
    ]
}

/// Render a Resolution into terminal lines. Pure; also used by tests.
pub fn resolution_lines(res: &Resolution) -> Vec<Line<'static>> {
    match res {
        Resolution::Memory {
            view,
            namespace_label,
            signal,
            content,
            content_hash,
            attestation_count,
            receipt_lines,
            prompt_links,
            first_seen,
            ..
        } => {
            let mut lines = decoded_lines(view);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "— local memory record —",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            )));
            lines.push(kv("ns label", namespace_label.clone(), CYAN));
            lines.push(kv("signal", format!("{:.4}", signal), GREEN));
            lines.push(kv("first seen", first_seen.clone(), DIM));
            lines.push(kv("content-hash", content_hash.clone(), DIM));
            lines.push(kv("attestations", format!("{}", attestation_count), Color::White));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("summary", Style::default().fg(DIM))));
            let clipped: String = content.chars().take(600).collect();
            lines.push(Line::from(Span::styled(clipped, Style::default().fg(Color::White))));
            if !prompt_links.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled("linked prompts", Style::default().fg(DIM))));
                for p in prompt_links {
                    lines.push(Line::from(vec![
                        Span::styled("  ⬢ ", Style::default().fg(GOLD)),
                        Span::styled(p.clone(), Style::default().fg(CYAN)),
                    ]));
                }
            }
            if !receipt_lines.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled("receipts (witness feed)", Style::default().fg(DIM))));
                for r in receipt_lines.iter().take(12) {
                    lines.push(Line::from(Span::styled(format!("  {}", r), Style::default().fg(DIM))));
                }
            }
            lines
        }
        Resolution::Prompt { view, linked_memories } => {
            let mut lines = decoded_lines(view);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "— prompt fingerprint → linked memories —",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            )));
            for (u, ns) in linked_memories {
                lines.push(Line::from(vec![
                    Span::styled("  ⬢ ", Style::default().fg(GOLD)),
                    Span::styled(format!("{:.8}…", u), Style::default().fg(Color::White)),
                    Span::styled(format!(" {}", ns), Style::default().fg(DIM)),
                ]));
            }
            lines
        }
        Resolution::Attestation { view, feed_lines } => {
            let mut lines = decoded_lines(view);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "— panel-plane attestations referencing this UUID —",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "  (full kind/count/receipt fields live on the witness plane,",
                Style::default().fg(DIM),
            )));
            lines.push(Line::from(Span::styled(
                "   imbuuid-mem Witness; the panel feed carries seq/witness/verdict/ts)",
                Style::default().fg(DIM),
            )));
            for f in feed_lines.iter().take(12) {
                lines.push(Line::from(Span::styled(format!("  {}", f), Style::default().fg(DIM))));
            }
            lines
        }
        Resolution::HonestMiss { view } => {
            let mut lines = decoded_lines(view);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "no local record — the UUID is the address, but nothing is stored at it here.",
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            )));
            lines
        }
        Resolution::DecodeError { input, error } => vec![
            kv("input", input.clone(), Color::White),
            Line::from(""),
            Line::from(vec![
                Span::styled("decode failed: ", Style::default().fg(RED).add_modifier(Modifier::BOLD)),
                Span::styled(error.clone(), Style::default().fg(RED)),
            ]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryStore;
    use imbuuid_mem::gyst::{encode_gyst, TYPE_MEMORY};

    fn seeded() -> InMemoryStore {
        InMemoryStore::seeded()
    }

    #[test]
    fn extract_uuids_from_sample_dossier() {
        let md = r#"# Dossier

Two real v8 UUIDs ride this dossier:

- `54001000-2000-8100-8000-000abc001111`
- 54111025-205b-810d-a038-000abc002222 trailing text

A version-4 UUID must NOT match:
- 550e8400-e29b-41d4-a716-446655440000

And garbage stays out: zzzz, 1234, deadbeef.
"#;
        let got = extract_uuids(md);
        assert_eq!(got.len(), 2, "got: {:?}", got);
        assert!(got.contains(&"54001000-2000-8100-8000-000abc001111".to_string()));
        assert!(got.contains(&"54111025-205b-810d-a038-000abc002222".to_string()));
        // dedupe: same uuid twice yields one entry
        let again = extract_uuids("54001000-2000-8100-8000-000abc001111 and 54001000-2000-8100-8000-000abc001111");
        assert_eq!(again.len(), 1);
    }

    #[test]
    fn extract_uuids_empty_and_no_match() {
        assert!(extract_uuids("").is_empty());
        assert!(extract_uuids("no uuids here at all").is_empty());
        assert!(extract_uuids("short 1234-5678").is_empty());
    }

    #[test]
    fn decode_resolve_round_trip_memory() {
        let store = seeded();
        let uuid = store.memories()[3].uuid.clone();
        let res = resolve(&uuid, &store);
        match &res {
            Resolution::Memory { view, namespace_label, .. } => {
                assert_eq!(view.uuid, uuid);
                assert_eq!(view.type_code, TYPE_MEMORY);
                assert_eq!(view.type_label, "MEMORY");
                assert_eq!(namespace_label, &store.memories()[3].namespace);
                assert!(view.ts_utc.contains("ts24 window-fit"));
            }
            other => panic!("expected Memory, got {:?}", std::mem::discriminant(other)),
        }
        // Rendered lines carry the record and no honest-miss text.
        let text: String = resolution_lines(&res)
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.clone())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(&uuid));
        assert!(!text.contains("no local record"));
    }

    #[test]
    fn decode_resolve_honest_miss() {
        let store = seeded();
        // A well-formed v8 MEMORY uuid the store never saw.
        let uuid = encode_gyst(TYPE_MEMORY, 0x123, 0x456789, 1, 2, 3, 0.5, PROV_CLI, 0xDEADBEEF);
        let res = resolve(&uuid, &store);
        assert!(matches!(res, Resolution::HonestMiss { .. }));
        let text: String = resolution_lines(&res)
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.clone())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(
            "no local record — the UUID is the address, but nothing is stored at it here."
        ));
        // Decoded fields still render honestly.
        assert!(text.contains("MEMORY (0x400)"));
        assert!(text.contains("0x123"));
    }

    #[test]
    fn decode_rejects_non_v8() {
        let res = resolve("550e8400-e29b-41d4-a716-446655440000", &seeded());
        assert!(matches!(res, Resolution::DecodeError { .. }));
        let res2 = resolve("not-a-uuid", &seeded());
        assert!(matches!(res2, Resolution::DecodeError { .. }));
    }

    #[test]
    fn type_and_prov_names_cover_contract() {
        assert_eq!(type_name(TYPE_MEMORY), "MEMORY");
        assert_eq!(type_name(TYPE_PROMPT), "PROMPT");
        assert_eq!(type_name(TYPE_ATTESTATION), "ATTESTATION");
        assert_eq!(type_name(TYPE_BINARY_PROVENANCE), "BINARY_PROVENANCE");
        assert_eq!(type_name(0x999), "UNKNOWN(0x999)");
        assert_eq!(prov_name(PROV_CLI), "CLI");
        assert_eq!(prov_name(PROV_ENCRYPTED), "ENCRYPTED");
        assert_eq!(prov_name(0x9), "UNKNOWN");
    }

    #[test]
    fn resolve_ts24_window_fit() {
        // ts24 near now reconstructs to now's window.
        let now = 1_760_700_000u64;
        let wrap = 1u64 << 24;
        let ts24 = (now % wrap) as u32;
        let got = resolve_ts24(ts24, now);
        assert!((got as i64 - now as i64).abs() < wrap as i64);
        // far-future field value folds back one window.
        let ahead = ((now % wrap) + wrap / 2 + 100) % wrap;
        let got2 = resolve_ts24(ahead as u32, now);
        assert!(got2 <= now + wrap / 2);
    }

    #[test]
    fn store_uuids_lists_memories_and_prompts() {
        let store = seeded();
        let uuids = store_uuids(&store);
        assert_eq!(uuids.len(), store.memories().len() + store.prompts.len());
        assert!(uuids.contains(&store.memories()[0].uuid));
        assert!(uuids.contains(&store.prompts[0].uuid));
    }

    #[test]
    fn prompt_resolves_to_linked_memories() {
        let store = seeded();
        let p = store.prompts[0].uuid.clone();
        let res = resolve(&p, &store);
        match res {
            Resolution::Prompt { view, linked_memories } => {
                assert_eq!(view.type_label, "PROMPT");
                assert!(!linked_memories.is_empty());
            }
            other => panic!("expected Prompt, got {:?}", std::mem::discriminant(&other)),
        }
    }

    #[test]
    fn load_dossier_uuids_reads_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("imbuuid-browser-test-dossier.md");
        let uuid = encode_gyst(TYPE_MEMORY, 0x1, 0x2, 0, 0, 0, 0.5, PROV_CLI, 42);
        std::fs::write(&path, format!("# d\n\ncarries {}\n", uuid)).unwrap();
        let got = load_dossier_uuids(path.to_str().unwrap()).unwrap();
        assert_eq!(got, vec![uuid]);
        assert!(load_dossier_uuids("/nonexistent/dossier.md").is_err());
        let _ = std::fs::remove_file(&path);
    }
}
