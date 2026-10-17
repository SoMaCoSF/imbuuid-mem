// file_id: SOM-DOC-10433-v1.0.0 (part: witness)
// name: witness.rs — Witness attestation plane
//
// THE WITNESS NEVER ISSUES. This type has NO insert/mint method — enforced
// structurally by simply not existing. It only attests to InsertOutcome
// values produced elsewhere (the roster witnesses, it never issues).
//
// Each attestation commits to the previous receipt:
//   receipt = sha256_hex(prev_receipt || uuid || kind || count || ts)
// with the genesis prev_receipt = "GENESIS". The chain is tamper-evident:
// any edit to an earlier attestation invalidates every later receipt.

use serde::{Deserialize, Serialize};

use crate::memory::InsertOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttestationKind {
    FirstObservation,
    ReObservation,
}

impl AttestationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttestationKind::FirstObservation => "FirstObservation",
            AttestationKind::ReObservation => "ReObservation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    pub uuid: String,
    pub kind: AttestationKind,
    pub observation_count: u64,
    pub timestamp_sec: u64,
    pub prev_receipt: String,
    pub receipt: String,
}

pub struct Witness {
    log: Vec<Attestation>,
    head: String,
}

impl Witness {
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            head: "GENESIS".to_string(),
        }
    }

    /// Attest to an InsertOutcome produced elsewhere. `Created` =>
    /// FirstObservation; `Duplicate` => ReObservation with the updated count.
    /// Appends to the hash chain and returns the new attestation.
    pub fn attest(&mut self, outcome: &InsertOutcome, timestamp_sec: u64) -> Attestation {
        let (uuid, kind, count) = match outcome {
            InsertOutcome::Created(rec) => (
                rec.uuid.clone(),
                AttestationKind::FirstObservation,
                rec.observation_count,
            ),
            InsertOutcome::Duplicate {
                uuid,
                observation_count,
            } => (
                uuid.clone(),
                AttestationKind::ReObservation,
                *observation_count,
            ),
        };
        let prev = self.head.clone();
        let material = format!("{}|{}|{}|{}|{}", prev, uuid, kind.as_str(), count, timestamp_sec);
        let receipt = crate::sha256_hex(material.as_bytes());
        let att = Attestation {
            uuid: uuid.clone(),
            kind,
            observation_count: count,
            timestamp_sec,
            prev_receipt: prev,
            receipt: receipt.clone(),
        };
        self.head = receipt;
        self.log.push(att.clone());
        tracing::info!(
            uuid = %uuid,
            kind = kind.as_str(),
            observation_count = count,
            "witness.attest"
        );
        att
    }

    pub fn attestations(&self) -> &[Attestation] {
        &self.log
    }

    /// Verify the chain: every receipt recomputes from its prev_receipt,
    /// and every link's prev_receipt equals the prior receipt ("GENESIS" first).
    pub fn verify_chain(&self) -> bool {
        let mut prev = "GENESIS";
        for att in &self.log {
            if att.prev_receipt != prev {
                return false;
            }
            let material = format!(
                "{}|{}|{}|{}|{}",
                att.prev_receipt,
                att.uuid,
                att.kind.as_str(),
                att.observation_count,
                att.timestamp_sec
            );
            if crate::sha256_hex(material.as_bytes()) != att.receipt {
                return false;
            }
            prev = &att.receipt;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gyst::PROV_CLI;
    use crate::memory::{InsertOpts, MemoryStore};

    fn opts() -> InsertOpts {
        InsertOpts {
            namespace_label: "w".to_string(),
            prompt_id: None,
            provenance: PROV_CLI,
            signal: 0.5,
            encrypted: false,
            hmac_key: None,
        }
    }

    #[test]
    fn attest_kinds_and_chain() {
        // Structural check: the Witness surface used here is only
        // new/attest/attestations/verify_chain — it exposes no mint/insert.
        let mut store = MemoryStore::open(None).unwrap();
        let mut wit = Witness::new();

        let o1 = store.insert("chain me", opts()).unwrap();
        let a1 = wit.attest(&o1, 1000);
        assert_eq!(a1.kind, AttestationKind::FirstObservation);
        assert_eq!(a1.prev_receipt, "GENESIS");

        let o2 = store.insert("chain me", opts()).unwrap();
        let a2 = wit.attest(&o2, 1001);
        assert_eq!(a2.kind, AttestationKind::ReObservation);
        assert_eq!(a2.observation_count, 2);
        assert_eq!(a2.prev_receipt, a1.receipt);

        assert_eq!(wit.attestations().len(), 2);
        assert!(wit.verify_chain());

        // Each receipt commits to its predecessor: recompute manually.
        let expected = crate::sha256_hex(
            format!("{}|{}|{}|{}|{}", a1.receipt, a2.uuid, "ReObservation", 2, 1001).as_bytes(),
        );
        assert_eq!(a2.receipt, expected);
    }

    #[test]
    fn tampered_chain_fails_verification() {
        let mut store = MemoryStore::open(None).unwrap();
        let mut wit = Witness::new();
        let o1 = store.insert("tamper", opts()).unwrap();
        wit.attest(&o1, 1000);
        wit.log[0].observation_count = 99; // corrupt the record
        assert!(!wit.verify_chain());
    }
}
