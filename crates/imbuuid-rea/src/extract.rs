// SOM-DOC-10435 (part) — REA extraction core: scan bytes, verify, decode.
//
// The UUIDv8 pattern bakes the version nibble (8) and the RFC 9562 variant
// bits ([89ab]) directly into the regex, so extraction SIMULTANEOUSLY
// verifies UUIDv8 compliance: a non-v8 / non-RFC-variant string can never
// match. decode_gyst then re-verifies version==8 && variant==2 as
// defense-in-depth before a hit is accepted.

use chrono::{DateTime, TimeZone, Utc};
use imbuuid_mem::gyst::{decode_gyst, DecodedGyst, TYPE_BINARY_PROVENANCE};
use regex::Regex;
use std::sync::OnceLock;

/// UUIDv8-shaped candidates only: version nibble 8, variant bits 10xx.
/// Lowercase hex: our build script mints lowercase, and rodata holds it verbatim.
pub const UUID_V8_PATTERN: &str =
    r"[0-9a-f]{8}-[0-9a-f]{4}-8[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";

/// Byte-stable provenance record our own binaries embed (see
/// imbuuid-panel/src/provenance.rs and the twin in this crate's main.rs).
/// Format: `IMBUUID_PROVENANCE_RECORD v=1 uuid=<uuid> builder=<user@host>
/// tree=<64-hex> ts=<unix>`. Joined to a UUID hit when present; binaries
/// without the record still yield uuid/timestamp/signal/provenance-code
/// from the UUID bits alone.
pub const PROVENANCE_RECORD_PATTERN: &str =
    r"IMBUUID_PROVENANCE_RECORD v=1 uuid=([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}) builder=([!-~]+) tree=([0-9a-f]{64}) ts=(\d+)";

fn uuid_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(UUID_V8_PATTERN).expect("valid UUIDv8 regex"))
}

fn record_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(PROVENANCE_RECORD_PATTERN).expect("valid record regex"))
}

/// One verified binary-provenance hit.
#[derive(Debug, Clone)]
pub struct ProvenanceHit {
    /// Canonical UUID string as found in the binary.
    pub uuid: String,
    /// Decoded GYST fields (type already verified == 0x403).
    pub decoded: DecodedGyst,
    /// Builder identity, when the binary embeds a provenance record.
    pub builder: Option<String>,
    /// Source-tree SHA-256, when the binary embeds a provenance record.
    pub source_tree_hash: Option<String>,
    /// Record unix timestamp, when the binary embeds a provenance record.
    pub record_timestamp: Option<u64>,
}

/// The 24-bit timestamp field wraps every 2^24 s (~194 days). Given the raw
/// field, reconstruct the most plausible absolute unix time: the multiple of
/// 2^24 nearest to now. Exact for recently-minted provenance UUIDs; labelled
/// as a reconstruction, never as ground truth.
pub fn reconstruct_timestamp(ts24: u32, now_sec: u64) -> u64 {
    const WINDOW: u64 = 1 << 24;
    let base = (now_sec / WINDOW) * WINDOW;
    let c = base + ts24 as u64;
    if c > now_sec && c - now_sec > WINDOW / 2 {
        c - WINDOW
    } else if now_sec > c && now_sec - c > WINDOW / 2 {
        c + WINDOW
    } else {
        c
    }
}

pub fn format_utc(ts: u64) -> String {
    match Utc.timestamp_opt(ts as i64, 0).single() {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => format!("<unrepresentable unix time {}>", ts),
    }
}

/// Scan raw bytes for TYPE_BINARY_PROVENANCE UUIDv8 records.
///
/// Returns verified hits in scan order, deduplicated by UUID string.
pub fn extract_provenance(bytes: &[u8]) -> Vec<ProvenanceHit> {
    let text = String::from_utf8_lossy(bytes);

    // Index embedded provenance records by UUID for the join.
    let mut records: std::collections::HashMap<String, (String, String, u64)> =
        std::collections::HashMap::new();
    for cap in record_re().captures_iter(&text) {
        let uuid = cap[1].to_string();
        let ts = cap[4].parse::<u64>().unwrap_or(0);
        records.insert(uuid, (cap[2].to_string(), cap[3].to_string(), ts));
    }

    let mut hits = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in uuid_re().find_iter(&text) {
        let s = m.as_str();
        if !seen.insert(s.to_string()) {
            continue;
        }
        let decoded = match decode_gyst(s) {
            Ok(d) => d,
            Err(_) => continue, // regex passed but decode failed: skip, never panic
        };
        // Defense-in-depth: the regex already enforces these, but a hit is
        // only a hit when the decoded bits agree.
        if decoded.version != 8 || decoded.variant != 2 {
            continue;
        }
        if decoded.type_code != TYPE_BINARY_PROVENANCE {
            continue;
        }
        let (builder, source_tree_hash, record_timestamp) = match records.get(s) {
            Some((b, t, ts)) => (Some(b.clone()), Some(t.clone()), Some(*ts)),
            None => (None, None, None),
        };
        hits.push(ProvenanceHit {
            uuid: s.to_string(),
            decoded,
            builder,
            source_tree_hash,
            record_timestamp,
        });
    }
    hits
}

/// Human-readable rendering of one hit for the CLI.
pub fn render_hit(hit: &ProvenanceHit, now_sec: u64) -> String {
    let d = &hit.decoded;
    let recon = reconstruct_timestamp(d.timestamp_sec, now_sec);
    let mut out = String::new();
    out.push_str(&format!("provenance UUID : {}\n", hit.uuid));
    out.push_str(&format!(
        "  decoded type  : 0x{:03x} (BINARY_PROVENANCE)\n",
        d.type_code
    ));
    out.push_str(&format!(
        "  timestamp     : {} s (decoded 24-bit field)\n",
        d.timestamp_sec
    ));
    out.push_str(&format!(
        "  timestamp_utc : ~{} (nearest 2^24 window to now — reconstruction)\n",
        format_utc(recon)
    ));
    match hit.record_timestamp {
        Some(ts) => out.push_str(&format!(
            "  record ts     : {} ({} — embedded, authoritative)\n",
            ts,
            format_utc(ts)
        )),
        None => out.push_str("  record ts     : <no embedded record>\n"),
    }
    out.push_str(&format!(
        "  builder       : {}\n",
        hit.builder.as_deref().unwrap_or("<no embedded record>")
    ));
    out.push_str(&format!(
        "  source tree   : {}\n",
        hit.source_tree_hash.as_deref().unwrap_or("<no embedded record>")
    ));
    out.push_str(&format!(
        "  signal        : {:.6} (raw {})\n",
        d.signal_normalized, d.signal_raw
    ));
    out.push_str(&format!("  provenance code: 0x{:x}\n", d.provenance));
    out.push_str(&format!(
        "  namespace     : 0x{:03x}  fractal depth/domain/gen: {}/{}/{}\n",
        d.namespace, d.fractal_depth, d.fractal_domain, d.fractal_generation
    ));
    let _ = DateTime::<Utc>::MIN_UTC; // keep chrono import honest in doc builds
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mint a UUID string with the exact layout build-support/provenance-build.rs uses.
    fn mint_test_uuid(type_code: u16, ts: u64, provenance: u8, signal_raw: u16) -> String {
        let ts24 = ts & 0xFFFFFF;
        let high: u64 = ((type_code as u64) << 52) | (ts24 << 16) | (8u64 << 12);
        let low: u64 =
            (2u64 << 62) | ((provenance as u64) << 58) | ((signal_raw as u64) << 42) | 0x2aaaaaaaaab;
        let u128 = ((high as u128) << 64) | (low as u128);
        let hex = format!("{:032x}", u128);
        format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }

    #[test]
    fn regex_matches_all_test_vectors() {
        // From ~/workspace/imbuuid-mem/test-vectors.json — all are v8/RFC-variant.
        let vectors = [
            "4005aae7-7800-8000-89ff-feaaaaaaaaab",
            "200190e7-7801-8060-a35c-26aaaaaaaaab",
            "3a0b89e7-7802-8010-9fff-feaaaaaaaaab",
            "3a1905e7-7803-8110-9cae-12aaaaaaaaab",
        ];
        for v in vectors {
            assert!(uuid_re().is_match(v), "should match {}", v);
        }
        // Non-v8 must NOT match: version 7, version 4, bad variant.
        assert!(!uuid_re().is_match("4005aae7-7800-7000-89ff-feaaaaaaaaab"));
        assert!(!uuid_re().is_match("4005aae7-7800-4000-89ff-feaaaaaaaaab"));
        assert!(!uuid_re().is_match("4005aae7-7800-8000-c9ff-feaaaaaaaaab"));
        assert!(!uuid_re().is_match("not-a-uuid-at-all-0000-000000000000"));
    }

    #[test]
    fn extracts_provenance_hit_with_record_join() {
        let ts: u64 = 1_760_700_000;
        let uuid = mint_test_uuid(TYPE_BINARY_PROVENANCE, ts, 0x2, 65535);
        let record = format!(
            "IMBUUID_PROVENANCE_RECORD v=1 uuid={} builder=tester@devbox tree={} ts={}",
            uuid,
            "ab".repeat(32),
            ts
        );
        let mut bytes = b"\x00\x7fELF-junk".to_vec();
        bytes.extend_from_slice(b"some rodata ... ");
        bytes.extend_from_slice(uuid.as_bytes());
        bytes.extend_from_slice(b" ... more bytes ... ");
        bytes.extend_from_slice(record.as_bytes());
        bytes.extend_from_slice(b"\x00\xff tail");

        let hits = extract_provenance(&bytes);
        assert_eq!(hits.len(), 1, "exactly one 0x403 hit");
        let h = &hits[0];
        assert_eq!(h.uuid, uuid);
        assert_eq!(h.decoded.type_code, 0x403);
        assert_eq!(h.decoded.timestamp_sec, (ts & 0xFFFFFF) as u32);
        assert_eq!(h.builder.as_deref(), Some("tester@devbox"));
        assert_eq!(h.source_tree_hash.as_deref(), Some("ab".repeat(32).as_str()));
        assert_eq!(h.record_timestamp, Some(ts));
        assert!((h.decoded.signal_normalized - 1.0).abs() < 1e-6);
        assert_eq!(h.decoded.provenance, 0x2);
    }

    #[test]
    fn ignores_non_provenance_v8_and_dedups() {
        let ts: u64 = 1_760_700_000;
        let other = mint_test_uuid(0x400, ts, 0x2, 32767); // memory type, not 0x403
        let prov = mint_test_uuid(TYPE_BINARY_PROVENANCE, ts, 0x2, 65535);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(other.as_bytes());
        bytes.extend_from_slice(b" ");
        bytes.extend_from_slice(prov.as_bytes());
        bytes.extend_from_slice(b" ");
        bytes.extend_from_slice(prov.as_bytes()); // duplicate occurrence
        let hits = extract_provenance(&bytes);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].uuid, prov);
    }

    #[test]
    fn empty_bytes_yield_no_hits() {
        assert!(extract_provenance(b"no uuids here, just text").is_empty());
        assert!(extract_provenance(b"").is_empty());
    }

    #[test]
    fn timestamp_reconstruction_roundtrips_recent_mint() {
        let now: u64 = 1_760_722_681;
        let ts24 = (now & 0xFFFFFF) as u32;
        assert_eq!(reconstruct_timestamp(ts24, now), now);
        // A field value far from now still lands in the nearest window.
        let recon = reconstruct_timestamp(0, now);
        assert!((recon as i64 - now as i64).abs() <= (1 << 23));
    }

    #[test]
    fn render_hit_contains_all_required_fields() {
        let ts: u64 = 1_760_700_000;
        let uuid = mint_test_uuid(TYPE_BINARY_PROVENANCE, ts, 0x2, 65535);
        let bytes = uuid.as_bytes().to_vec();
        let hits = extract_provenance(&bytes);
        let rendered = render_hit(&hits[0], ts + 3600);
        for needle in [
            "provenance UUID",
            "0x403",
            "timestamp",
            "builder",
            "source tree",
            "signal",
            "provenance code",
        ] {
            assert!(rendered.contains(needle), "missing {}", needle);
        }
    }
}
