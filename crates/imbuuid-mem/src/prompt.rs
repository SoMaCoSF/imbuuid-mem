// file_id: SOM-DOC-10433-v1.0.0 (part: prompt)
// name: prompt.rs — prompt fingerprinting
//
// hash(prompt) linked to every memory's UUID + provenance. The fingerprint
// is itself a TYPE_PROMPT UUID whose rand42 field carries the content hash
// (same canonical scheme as memory.rs: format!("{}|{}", TYPE_PROMPT, norm)).
// The timestamp is PINNED to 0 so identical prompt text (+provenance)
// always produces the identical fingerprint UUID — it is content-addressed,
// not time-addressed. Provenance rides the prov nibble.

use serde::{Deserialize, Serialize};

use crate::gyst::{encode_gyst, fnv1a12, TYPE_PROMPT};
use crate::memory::normalize;

/// Deterministic prompt fingerprint UUID (TYPE_PROMPT, ts pinned to 0,
/// content hash in rand42, signal 1.0 — the prompt is asserted as identity).
pub fn fingerprint_prompt(prompt_text: &str, provenance: u8) -> String {
    let norm = normalize(prompt_text);
    let canonical = format!("{}|{}", TYPE_PROMPT, norm);
    let hash = crate::sha256_low42(canonical.as_bytes());
    // Namespace also derives from normalized text: the fingerprint is fully
    // canonical — whitespace variation never changes it.
    let uuid = encode_gyst(TYPE_PROMPT, fnv1a12(&norm), 0, 0, 0, 0, 1.0, provenance, hash);
    tracing::info!(uuid = %uuid, provenance = provenance, "prompt.fingerprint");
    uuid
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptLink {
    pub prompt_uuid: String,
    pub memory_uuid: String,
}

/// Link a prompt fingerprint to a memory UUID.
pub fn link_prompt_memory(prompt_uuid: &str, memory_uuid: &str) -> PromptLink {
    tracing::info!(prompt_uuid = %prompt_uuid, memory_uuid = %memory_uuid, "prompt.link");
    PromptLink {
        prompt_uuid: prompt_uuid.to_string(),
        memory_uuid: memory_uuid.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gyst::{decode_gyst, PROV_AGENT};

    #[test]
    fn same_prompt_same_fingerprint() {
        let a = fingerprint_prompt("summarize the trade log", PROV_AGENT);
        let b = fingerprint_prompt("summarize the trade log", PROV_AGENT);
        assert_eq!(a, b);
        let d = decode_gyst(&a).unwrap();
        assert_eq!(d.type_code, TYPE_PROMPT);
        assert_eq!(d.provenance, PROV_AGENT);
        assert_eq!(d.version, 8);
        assert_eq!(d.variant, 2);
        assert_eq!(d.timestamp_sec, 0); // pinned: content-addressed, not time-addressed
    }

    #[test]
    fn whitespace_normalized_and_text_sensitive() {
        let a = fingerprint_prompt("summarize the trade log", PROV_AGENT);
        let b = fingerprint_prompt("  summarize   the trade log ", PROV_AGENT);
        assert_eq!(a, b); // same canonical form
        let c = fingerprint_prompt("summarize the trade log!", PROV_AGENT);
        assert_ne!(a, c);
        let d = fingerprint_prompt("summarize the trade log", 0x1);
        assert_ne!(a, d); // provenance is part of the fingerprint
    }

    #[test]
    fn link_carries_both_uuids() {
        let p = fingerprint_prompt("q", PROV_AGENT);
        let l = link_prompt_memory(&p, "deadbeef-memory-uuid");
        assert_eq!(l.prompt_uuid, p);
        assert_eq!(l.memory_uuid, "deadbeef-memory-uuid");
    }
}
