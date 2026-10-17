// file_id: SOM-DOC-10433-v1.0.0 (part: lib)
// name: lib.rs — crate root, re-exports all six modules
//
// imbuuid-mem: UUID-substrate memory layer for AI agents. Local-first —
// no network access at runtime, ever. Storage/ingest answer is always the
// UUID DNA substrate: every record is UUID-keyed and content-addressed.

pub mod gyst;
pub mod memory;
pub mod prompt;
pub mod provenance;
pub mod retrieval;
pub mod witness;

/// SHA-256 of `data`, hex-encoded (64 lowercase chars). Used for receipts
/// (witness) and the source-tree hash (provenance build script shape).
pub(crate) fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let out = Sha256::digest(data);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for b in out.iter() {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xF) as usize] as char);
    }
    s
}

/// Low 42 bits of SHA-256(`data`) — the 42-bit content-address slot that the
/// codetable repurposes into the UUID rand42 field for memory/prompt types.
pub(crate) fn sha256_low42(data: &[u8]) -> u64 {
    use sha2::{Digest, Sha256};
    let out = Sha256::digest(data);
    let mut r: u64 = 0;
    for b in out.iter().skip(24) {
        r = (r << 8) | (*b as u64);
    }
    r & ((1u64 << 42) - 1)
}
