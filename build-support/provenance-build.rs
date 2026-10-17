// Shared build-time provenance minter for imbuuid-mem binaries.
// Intended to be pulled into each crate's build.rs via:
//     include!("../../build-support/provenance-build.rs");
// (path relative to the including build.rs in crates/<name>/)
//
// Mints a GYST UUIDv8 (type 0x403 BINARY_PROVENANCE) committing to
// (source-tree SHA-256 || unix timestamp || builder identity) and exposes it
// to the crate at compile time via `env!("IMBUUID_PROVENANCE_UUID")`.
//
// NOTE: this file carries its own minimal encoder and does NOT depend on the
// imbuuid-mem crate, so build scripts stay acyclic. The `imbuuid-rea`
// extractor decodes these UUIDs with the full crate implementation —
// independent implementation on each side is the point (REA-provable
// timestamped binary provenance).
//
// Requires `sha2` in the crate's [build-dependencies].
//
// Part of SOM-DOC-10435.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const TYPE_BINARY_PROVENANCE: u16 = 0x403;
const PROV_CLI: u8 = 0x2;

fn fnv1a12(label: &str) -> u16 {
    let mut h: u32 = 0x811c9dc5;
    for b in label.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    ((h ^ ((h >> 12) & 0xFFF)) & 0xFFF) as u16
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let out = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for b in out.iter() {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xF) as usize] as char);
    }
    s
}

fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|x| x.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            if p.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            collect_sources(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");

    // 1. Source-tree hash: sha256 over sorted (relpath NUL contents NUL).
    let mut sources = Vec::new();
    collect_sources(&manifest_dir.join("src"), &mut sources);
    let mut tree_input = Vec::new();
    for p in &sources {
        let rel = p.strip_prefix(&manifest_dir).unwrap_or(p);
        tree_input.extend_from_slice(rel.to_string_lossy().as_bytes());
        tree_input.push(0);
        tree_input.extend_from_slice(&fs::read(p).unwrap_or_default());
        tree_input.push(0);
    }
    let tree_hash = sha256_hex(&tree_input);

    // 2. Timestamp + builder identity.
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let builder = env::var("IMBUUID_BUILDER").unwrap_or_else(|_| {
        let user = env::var("USER").unwrap_or_else(|_| "unknown".into());
        let host = fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "unknown\n".into());
        format!("{}@{}", user, host.trim())
    });

    // 3. Mint the provenance UUID (minimal inline encoder, GYST v2.1 layout).
    let ns = fnv1a12(&format!("imbuuid-build:{}", builder));
    let ts24 = ts & 0xFFFFFF;
    let high: u64 = ((TYPE_BINARY_PROVENANCE as u64) << 52)
        | ((ns as u64) << 40)
        | (ts24 << 16)
        | (8u64 << 12) // ver = 8 (UUIDv8)
        | 0u64; // fractal = depth 0, domain 0, generation 0
               // rand42: first 42 bits of sha256(tree_hash) — deterministic per tree
    let rand42 = {
        let d = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(tree_hash.as_bytes());
            h.finalize()
        };
        let mut r: u64 = 0;
        for i in 0..6 {
            r = (r << 8) | d[i] as u64;
        }
        r & ((1u64 << 42) - 1)
    };
    let sig: u64 = 0xFFFF; // signal 1.0 — provenance asserted at full confidence
    let low: u64 = (2u64 << 62) // variant 0b10 (RFC 9562)
        | ((PROV_CLI as u64) << 58)
        | (sig << 42)
        | rand42;
    let u128 = ((high as u128) << 64) | (low as u128);
    let hex = format!("{:032x}", u128);
    let uuid = format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    );

    println!("cargo:rustc-env=IMBUUID_PROVENANCE_UUID={}", uuid);
    println!("cargo:rustc-env=IMBUUID_SOURCE_TREE_HASH={}", tree_hash);
    println!("cargo:rustc-env=IMBUUID_BUILD_TIMESTAMP={}", ts);
    println!("cargo:rustc-env=IMBUUID_BUILDER={}", builder);
    println!("cargo:warning=imbuuid provenance {}", uuid);
}
