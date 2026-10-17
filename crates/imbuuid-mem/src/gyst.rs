// file_id: SOM-DOC-10433-v1.0.0 (part: gyst)
// name: gyst.rs — GYST UUIDv8 v2.1 codec (byte-exact port of uuid_service_turboquant.py)
//
// Bit layout, 128 bits MSB->LSB:
//   type(12) | ns(12) | ts(24) | ver=8(4) | fractal(12) | variant=0b10(2) |
//   prov(4) | sig(16) | rand(42)
// ver nibble MUST be 8 at bits 76-79; variant MUST be 0b10 at bits 62-63
// (RFC 9562 UUIDv8). fracta(12) = depth(4)|domain(4)|generation(4).
//
// 0x400-0x403 (TYPE_MEMORY / TYPE_PROMPT / TYPE_ATTESTATION /
// TYPE_BINARY_PROVENANCE) are PROVISIONAL codetable additions for this
// crate — not yet ratified in the canonical codetable.

use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

// ---- Provenance codes (mirrors gyst.ts / uuid_service_turboquant.py PROVENANCE) ----
pub const PROV_UNKNOWN: u8 = 0x0;
pub const PROV_DEXTER: u8 = 0x1;
pub const PROV_CLI: u8 = 0x2;
pub const PROV_CLAUDE: u8 = 0x3;
pub const PROV_DASHBOARD: u8 = 0x4;
pub const PROV_REGISTRY: u8 = 0x5;
pub const PROV_AGENT: u8 = 0x6;
pub const PROV_POLY_MAKER: u8 = 0x7;
pub const PROV_TURBO_QUANT: u8 = 0x8;
pub const PROV_ENCRYPTED: u8 = 0xF;

// ---- Type codes ----
pub const TYPE_TURBO_KV: u16 = 0x200;
pub const TYPE_TURBO_ATTN: u16 = 0x201;
pub const TYPE_TURBO_LAYER: u16 = 0x202;
pub const TYPE_POLY_MARKET: u16 = 0x3A0;
pub const TYPE_POLY_OUTCOME_QUOTE: u16 = 0x3A1;
/// PROVISIONAL codetable additions for the memory estate (0x400-0x403).
pub const TYPE_MEMORY: u16 = 0x400;
pub const TYPE_PROMPT: u16 = 0x401;
pub const TYPE_ATTESTATION: u16 = 0x402;
pub const TYPE_BINARY_PROVENANCE: u16 = 0x403;

/// 12-bit FNV-1a namespace hash — byte-exact mirror of gyst.ts hashNamespace12.
pub fn fnv1a12(label: &str) -> u16 {
    let mut h: u32 = 0x811c9dc5;
    for b in label.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    ((h ^ ((h >> 12) & 0xFFF)) & 0xFFF) as u16
}

/// Encode a 128-bit GYST UUIDv8 string. Byte-exact with `encodeGYST` in
/// gyst.ts and `encode_gyst` in uuid_service_turboquant.py.
///
/// `signal` is clamped to [0,1] and quantized to 16 bits exactly like the
/// Python reference: `int(clamp(signal) * 0xFFFF)`. `rand42` is masked to
/// 42 bits; `timestamp_sec` to 24 bits; `type_code`/`namespace` to 12 bits.
pub fn encode_gyst(
    type_code: u16,
    namespace: u16,
    timestamp_sec: u64,
    fractal_depth: u8,
    fractal_domain: u8,
    fractal_generation: u8,
    signal: f32,
    provenance: u8,
    rand42: u64,
) -> String {
    let ts = timestamp_sec & 0xFF_FFFF;
    let fractal = (((fractal_depth & 0xF) as u64) << 8)
        | (((fractal_domain & 0xF) as u64) << 4)
        | ((fractal_generation & 0xF) as u64);

    let high: u64 = (((type_code & 0xFFF) as u64) << 52)
        | (((namespace & 0xFFF) as u64) << 40)
        | (ts << 16)
        | (8u64 << 12) // ver = 8 (UUIDv8, bits 76-79)
        | fractal;

    let sig_q = (signal.clamp(0.0, 1.0) * 65535.0) as u64;
    let low: u64 = (2u64 << 62) // variant 0b10 (RFC 9562, bits 62-63)
        | (((provenance & 0xF) as u64) << 58)
        | (sig_q << 42)
        | (rand42 & ((1u64 << 42) - 1));

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

/// Mint a fresh GYST UUIDv8: `rand42` from the OS CSPRNG via getrandom,
/// timestamp = now (masked to 24 bits).
pub fn mint_gyst(
    type_code: u16,
    namespace: u16,
    fractal_depth: u8,
    fractal_domain: u8,
    fractal_generation: u8,
    signal: f32,
    provenance: u8,
) -> String {
    let rand42 = getrandom::u64().expect("getrandom: OS RNG unavailable") & ((1u64 << 42) - 1);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before epoch")
        .as_secs();
    let uuid = encode_gyst(
        type_code,
        namespace,
        ts,
        fractal_depth,
        fractal_domain,
        fractal_generation,
        signal,
        provenance,
        rand42,
    );
    tracing::info!(uuid = %uuid, type_code = type_code, "gyst.mint");
    uuid
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedGyst {
    pub type_code: u16,
    pub namespace: u16,
    pub timestamp_sec: u32,
    pub version: u8,
    pub fractal_depth: u8,
    pub fractal_domain: u8,
    pub fractal_generation: u8,
    pub variant: u8,
    pub provenance: u8,
    pub signal_raw: u16,
    pub signal_normalized: f32,
    pub random: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GystError {
    #[error("invalid UUID length {0}: expected 32 hex chars")]
    InvalidLength(usize),
    #[error("invalid hex characters in UUID")]
    InvalidHex,
}

/// Decode a GYST UUIDv8 string back into its fields. Accepts hyphenated or
/// bare 32-hex form (case-insensitive).
pub fn decode_gyst(s: &str) -> Result<DecodedGyst, GystError> {
    let clean: String = s.chars().filter(|c| *c != '-').collect();
    if clean.len() != 32 {
        return Err(GystError::InvalidLength(clean.len()));
    }
    if !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(GystError::InvalidHex);
    }
    let u128 = u128::from_str_radix(&clean, 16).map_err(|_| GystError::InvalidHex)?;
    let high = (u128 >> 64) as u64;
    let low = (u128 & ((1u128 << 64) - 1)) as u64;

    let fractal = high & 0xFFF;
    let sig = ((low >> 42) & 0xFFFF) as u16;

    Ok(DecodedGyst {
        type_code: ((high >> 52) & 0xFFF) as u16,
        namespace: ((high >> 40) & 0xFFF) as u16,
        timestamp_sec: ((high >> 16) & 0xFF_FFFF) as u32,
        version: ((high >> 12) & 0xF) as u8,
        fractal_depth: ((fractal >> 8) & 0xF) as u8,
        fractal_domain: ((fractal >> 4) & 0xF) as u8,
        fractal_generation: (fractal & 0xF) as u8,
        variant: ((low >> 62) & 0x3) as u8,
        provenance: ((low >> 58) & 0xF) as u8,
        signal_raw: sig,
        signal_normalized: (sig as f32) / 65535.0,
        random: low & ((1u64 << 42) - 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a12_matches_reference_labels() {
        // Namespace labels asserted in test-vectors.json.
        assert_eq!(fnv1a12("test-ns"), 1450);
        assert_eq!(fnv1a12("turboquant:llama3-70b:layer7"), 400);
        assert_eq!(fnv1a12("polymarket:trump-2028"), 2953);
        assert_eq!(fnv1a12("polymarket:quote:trump-2028"), 2309);
    }

    #[test]
    fn signal_quantization_matches_python_truncation() {
        // Python: int(clamp(sig,0,1) * 0xFFFF) — truncation, not rounding.
        let u = encode_gyst(0x400, 0, 0, 0, 0, 0, 0.5, 0, 0);
        assert_eq!(decode_gyst(&u).unwrap().signal_raw, 32767);
        let u = encode_gyst(0x400, 0, 0, 0, 0, 0, 0.84, 0, 0);
        assert_eq!(decode_gyst(&u).unwrap().signal_raw, 55049);
        let u = encode_gyst(0x400, 0, 0, 0, 0, 0, 1.7, 0, 0); // clamped to 1.0
        assert_eq!(decode_gyst(&u).unwrap().signal_raw, 65535);
        let u = encode_gyst(0x400, 0, 0, 0, 0, 0, -0.5, 0, 0); // clamped to 0.0
        assert_eq!(decode_gyst(&u).unwrap().signal_raw, 0);
    }

    #[test]
    fn decode_rejects_malformed() {
        assert_eq!(decode_gyst("abc").unwrap_err(), GystError::InvalidLength(3));
        assert_eq!(
            decode_gyst("zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz")
                .unwrap_err(),
            GystError::InvalidHex
        );
    }

    #[test]
    fn uuidv8_bits_set_on_encode() {
        let u = encode_gyst(0x400, 0xABC, 0x123456, 3, 7, 15, 0.25, PROV_CLAUDE, 0x123456789A);
        let d = decode_gyst(&u).unwrap();
        assert_eq!(d.version, 8);
        assert_eq!(d.variant, 2);
        assert_eq!(d.type_code, 0x400);
        assert_eq!(d.namespace, 0xABC);
        assert_eq!(d.timestamp_sec, 0x123456);
        assert_eq!(d.fractal_depth, 3);
        assert_eq!(d.fractal_domain, 7);
        assert_eq!(d.fractal_generation, 15);
        assert_eq!(d.provenance, PROV_CLAUDE);
        assert_eq!(d.random, 0x123456789A & ((1u64 << 42) - 1));
        // rand42 masked to 42 bits: passing 48 bits must not leak into sig field.
        let u2 = encode_gyst(0x400, 0, 0, 0, 0, 0, 0.0, 0, 0xFFFF_FFFF_FFFF);
        assert_eq!(decode_gyst(&u2).unwrap().signal_raw, 0);
        assert_eq!(decode_gyst(&u2).unwrap().random, (1u64 << 42) - 1);
    }
}
