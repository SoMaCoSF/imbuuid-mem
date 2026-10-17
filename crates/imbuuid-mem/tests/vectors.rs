// file_id: SOM-DOC-10433-v1.0.0 (part: tests/vectors)
// name: tests/vectors.rs — byte-exactness against test-vectors.json
//
// Every vector in the workspace-root test-vectors.json was generated with a
// fixed rand42 (0x2AAAAAAAAAB & ((1<<42)-1)). We re-encode with the explicit
// rand42 and require the UUID string to match byte-for-byte, then decode and
// require every field to match.

use imbuuid_mem::gyst::{decode_gyst, encode_gyst, fnv1a12};
use serde::Deserialize;

// Relative to this file: crates/imbuuid-mem/tests/ -> workspace root.
const VECTORS: &[u8] = include_bytes!("../../../test-vectors.json");

#[derive(Debug, Deserialize)]
struct Inputs {
    type_code: u16,
    namespace: u16,
    namespace_label: String,
    timestamp_sec: u64,
    fractal_depth: u8,
    fractal_domain: u8,
    fractal_generation: u8,
    signal: f32,
    provenance: u8,
    rand42: u64,
}

#[derive(Debug, Deserialize)]
struct DecodedExpected {
    type_code: u16,
    namespace: u16,
    timestamp_sec: u32,
    version: u8,
    fractal_depth: u8,
    fractal_domain: u8,
    fractal_generation: u8,
    variant: u8,
    provenance: u8,
    signal_raw: u16,
    signal_normalized: f64,
    random: u64,
}

#[derive(Debug, Deserialize)]
struct Vector {
    inputs: Inputs,
    uuid: String,
    decoded: DecodedExpected,
}

#[test]
fn vectors_byte_exact() {
    let vecs: Vec<Vector> = serde_json::from_slice(VECTORS).expect("parse test-vectors.json");
    assert_eq!(vecs.len(), 4, "expected all 4 reference vectors");

    for (i, v) in vecs.iter().enumerate() {
        // The namespace label hashes to the vector's namespace.
        assert_eq!(
            fnv1a12(&v.inputs.namespace_label),
            v.inputs.namespace,
            "vector {i}: fnv1a12 label mismatch"
        );

        // Byte-exact encode with the explicit rand42.
        let enc = encode_gyst(
            v.inputs.type_code,
            v.inputs.namespace,
            v.inputs.timestamp_sec,
            v.inputs.fractal_depth,
            v.inputs.fractal_domain,
            v.inputs.fractal_generation,
            v.inputs.signal,
            v.inputs.provenance,
            v.inputs.rand42,
        );
        assert_eq!(enc, v.uuid, "vector {i}: encode mismatch");

        // Decode round-trip: every field matches.
        let d = decode_gyst(&v.uuid).expect("decode");
        let e = &v.decoded;
        assert_eq!(d.type_code, e.type_code, "vector {i}: type_code");
        assert_eq!(d.namespace, e.namespace, "vector {i}: namespace");
        assert_eq!(d.timestamp_sec, e.timestamp_sec, "vector {i}: timestamp_sec");
        assert_eq!(d.version, e.version, "vector {i}: version");
        assert_eq!(d.fractal_depth, e.fractal_depth, "vector {i}: fractal_depth");
        assert_eq!(d.fractal_domain, e.fractal_domain, "vector {i}: fractal_domain");
        assert_eq!(d.fractal_generation, e.fractal_generation, "vector {i}: fractal_generation");
        assert_eq!(d.variant, e.variant, "vector {i}: variant");
        assert_eq!(d.provenance, e.provenance, "vector {i}: provenance");
        assert_eq!(d.signal_raw, e.signal_raw, "vector {i}: signal_raw");
        assert!(
            (d.signal_normalized as f64 - e.signal_normalized).abs() < 1e-6,
            "vector {i}: signal_normalized {} vs {}",
            d.signal_normalized,
            e.signal_normalized
        );
        assert_eq!(d.random, e.random, "vector {i}: random");
    }
}
