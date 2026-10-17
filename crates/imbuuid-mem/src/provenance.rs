// file_id: SOM-DOC-10433-v1.0.0 (part: provenance)
// name: provenance.rs — build-time binary provenance
//
// Reads compile-time env emitted by build-support/provenance-build.rs
// (included via this crate's build.rs):
//   IMBUUID_PROVENANCE_UUID   — TYPE_BINARY_PROVENANCE (0x403) GYST UUIDv8
//                               committing to (source-tree hash || timestamp || builder)
//   IMBUUID_SOURCE_TREE_HASH  — SHA-256 over sorted (relpath NUL contents NUL)
//   IMBUUID_BUILD_TIMESTAMP   — unix seconds at build time
//   IMBUUID_BUILDER           — user@host (or $IMBUUID_BUILDER override)
//
// Returns None when any of the env vars is absent (e.g. the build script
// was not run). The minter's encoder is deliberately independent of the
// gyst module in this crate; `imbuuid-rea` decodes these UUIDs with the
// full implementation — independent implementation on each side is the
// point (REA-provable timestamped binary provenance).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryProvenance {
    pub uuid: String,
    pub source_tree_hash: String,
    pub timestamp_sec: u64,
    pub builder: String,
}

/// Build-time binary provenance, or None if the build script did not emit it.
pub fn binary_provenance() -> Option<BinaryProvenance> {
    let uuid = option_env!("IMBUUID_PROVENANCE_UUID")?.to_string();
    let source_tree_hash = option_env!("IMBUUID_SOURCE_TREE_HASH")?.to_string();
    let timestamp_sec = option_env!("IMBUUID_BUILD_TIMESTAMP")?
        .parse::<u64>()
        .ok()?;
    let builder = option_env!("IMBUUID_BUILDER")?.to_string();
    Some(BinaryProvenance {
        uuid,
        source_tree_hash,
        timestamp_sec,
        builder,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_present_when_build_script_ran() {
        // The crate ships a build.rs that includes build-support/provenance-build.rs,
        // so under a normal cargo build all four env vars are set.
        let p = binary_provenance();
        assert!(p.is_some(), "build script should have emitted provenance env");
        let p = p.unwrap();
        let d = crate::gyst::decode_gyst(&p.uuid).unwrap();
        assert_eq!(d.type_code, crate::gyst::TYPE_BINARY_PROVENANCE);
        assert_eq!(d.version, 8);
        assert_eq!(d.variant, 2);
        assert_eq!(p.source_tree_hash.len(), 64);
    }
}
