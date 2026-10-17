// SOM-DOC-10435 (part) — embedded build provenance for imbuuid-panel.
//
// build.rs mints a GYST UUIDv8 type 0x403 at compile time committing to
// (source-tree SHA-256 || unix timestamp || builder) and exposes it via
// cargo:rustc-env. This module surfaces it at runtime and embeds the
// byte-stable record line the imbuuid-rea extractor scans for.

/// Minted provenance UUID (canonical hyphenated lowercase).
pub const PROVENANCE_UUID: &str = env!("IMBUUID_PROVENANCE_UUID");
/// SHA-256 over sorted (relpath NUL contents NUL) of this crate's src tree.
pub const SOURCE_TREE_HASH: &str = env!("IMBUUID_SOURCE_TREE_HASH");
/// Unix seconds at mint time.
pub const BUILD_TIMESTAMP: &str = env!("IMBUUID_BUILD_TIMESTAMP");
/// Builder identity (`$IMBUUID_BUILDER` or `user@host` at compile time).
pub const BUILDER: &str = env!("IMBUUID_BUILDER");

/// Byte-stable record the REA extractor regex-scans for. `#[used]` keeps it
/// in .rodata even though no Rust code reads it — the extractor is the reader.
/// Field order and separators are the contract; do not restyle.
#[used]
static PROVENANCE_RECORD: &str = concat!(
    "IMBUUID_PROVENANCE_RECORD v=1 uuid=",
    env!("IMBUUID_PROVENANCE_UUID"),
    " builder=",
    env!("IMBUUID_BUILDER"),
    " tree=",
    env!("IMBUUID_SOURCE_TREE_HASH"),
    " ts=",
    env!("IMBUUID_BUILD_TIMESTAMP"),
);

/// The embedded record line (for tests; the extractor reads the binary).
pub fn provenance_record() -> &'static str {
    PROVENANCE_RECORD
}

/// Short 8-char prefix for chrome (headers, window titles).
pub fn provenance_short() -> String {
    PROVENANCE_UUID.chars().take(8).collect()
}

/// Print full provenance: this binary's OWN embedded provenance first
/// (env! expands at this crate's compile time), then the transitively-linked
/// imbuuid-mem library's provenance. Used by `--provenance`.
pub fn print_provenance() {
    println!("imbuuid-panel binary provenance (own build):");
    println!("  IMBUUID_PROVENANCE_UUID  = {}", PROVENANCE_UUID);
    println!("  IMBUUID_SOURCE_TREE_HASH = {}", SOURCE_TREE_HASH);
    println!("  IMBUUID_BUILD_TIMESTAMP  = {}", BUILD_TIMESTAMP);
    println!("  IMBUUID_BUILDER          = {}", BUILDER);
    println!("  record line              = {}", provenance_record());
    println!("linked imbuuid-mem library provenance (transitive):");
    match imbuuid_mem::provenance::binary_provenance() {
        Some(bp) => println!("{:#?}", bp),
        None => println!("<none: build script did not emit provenance env>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_line_matches_extractor_contract() {
        let r = provenance_record();
        assert!(r.starts_with("IMBUUID_PROVENANCE_RECORD v=1 uuid="));
        assert!(r.contains(" builder="));
        assert!(r.contains(" tree="));
        assert!(r.contains(" ts="));
        // The UUID inside the record is itself UUIDv8-shaped (version 8,
        // RFC 9562 variant) — the extractor's regex must match it.
        let uuid_part = r
            .strip_prefix("IMBUUID_PROVENANCE_RECORD v=1 uuid=")
            .unwrap()
            .split(' ')
            .next()
            .unwrap();
        let re = regex_lite_shape(uuid_part);
        assert!(re, "record uuid not v8-shaped: {}", uuid_part);
        assert_eq!(uuid_part, PROVENANCE_UUID);
    }

    /// Minimal v8-shape check without pulling regex into this module's tests.
    fn regex_lite_shape(s: &str) -> bool {
        let b = s.as_bytes();
        s.len() == 36
            && b[8] == b'-'
            && b[13] == b'-'
            && b[14] == b'8'
            && b[18] == b'-'
            && matches!(b[19], b'8' | b'9' | b'a' | b'b')
            && b[23] == b'-'
            && s.chars()
                .all(|c| c.is_ascii_hexdigit() || c == '-')
    }

    #[test]
    fn env_constants_are_sane() {
        assert_eq!(PROVENANCE_UUID.len(), 36);
        assert_eq!(SOURCE_TREE_HASH.len(), 64);
        assert!(SOURCE_TREE_HASH.chars().all(|c| c.is_ascii_hexdigit()));
        let ts: u64 = BUILD_TIMESTAMP.parse().expect("build timestamp parses");
        assert!(ts > 1_700_000_000, "sane unix time");
        assert!(!BUILDER.is_empty());
        assert_eq!(provenance_short().len(), 8);
    }
}
