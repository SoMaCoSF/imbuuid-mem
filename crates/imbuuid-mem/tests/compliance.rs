// file_id: SOM-DOC-10433-v1.0.0 (part: tests/compliance)
// name: tests/compliance.rs — UUIDv8 (RFC 9562) compliance across mint paths
//
// Mint 1,000 UUIDs across the type codes the crate serves and assert
// version==8 and variant==0b10 on every single one, via decode.

use imbuuid_mem::gyst::{decode_gyst, fnv1a12, mint_gyst, PROV_AGENT, PROV_CLI, PROV_ENCRYPTED,
    TYPE_ATTESTATION, TYPE_BINARY_PROVENANCE, TYPE_MEMORY, TYPE_POLY_MARKET, TYPE_PROMPT,
    TYPE_TURBO_KV};

#[test]
fn mint_1000_uuidv8_compliant() {
    let types = [
        TYPE_MEMORY,
        TYPE_PROMPT,
        TYPE_ATTESTATION,
        TYPE_BINARY_PROVENANCE,
        TYPE_TURBO_KV,
        TYPE_POLY_MARKET,
    ];
    let provs = [PROV_CLI, PROV_AGENT, PROV_ENCRYPTED];
    let mut seen = std::collections::HashSet::new();
    for i in 0..1000 {
        let uuid = mint_gyst(
            types[i % types.len()],
            fnv1a12(&format!("compliance-ns-{}", i % 7)),
            (i % 16) as u8,
            (i % 16) as u8,
            (i % 16) as u8,
            (i as f32 % 101.0) / 100.0,
            provs[i % provs.len()],
        );
        let d = decode_gyst(&uuid).expect("minted uuid must decode");
        assert_eq!(d.version, 8, "mint {i}: version != 8");
        assert_eq!(d.variant, 2, "mint {i}: variant != 0b10");
        assert!(
            seen.insert(uuid),
            "mint {i}: duplicate uuid (rand42 collision)"
        );
    }
    assert_eq!(seen.len(), 1000);
}
