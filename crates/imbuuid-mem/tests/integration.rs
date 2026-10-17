// file_id: SOM-DOC-10433-v1.0.0 (part: tests/integration)
// name: tests/integration.rs — cross-module flows: persistence + witness wiring

use imbuuid_mem::gyst::PROV_CLI;
use imbuuid_mem::memory::{InsertOpts, InsertOutcome, MemoryStore};
use imbuuid_mem::prompt::{fingerprint_prompt, link_prompt_memory};
use imbuuid_mem::witness::Witness;

fn opts() -> InsertOpts {
    InsertOpts {
        namespace_label: "integ".to_string(),
        prompt_id: None,
        provenance: PROV_CLI,
        signal: 0.7,
        encrypted: false,
        hmac_key: None,
    }
}

#[test]
fn persistence_round_trip() {
    let dir = std::env::temp_dir().join(format!("imbuuid-mem-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let prompt = fingerprint_prompt("persist this session", PROV_CLI);
    let mut o = opts();
    o.prompt_id = Some(prompt.clone());
    let uuid = {
        let mut s = MemoryStore::open(Some(dir.clone())).unwrap();
        let uuid = match s.insert("durable memory", o).unwrap() {
            InsertOutcome::Created(r) => r.uuid,
            _ => panic!(),
        };
        s.flush().unwrap();
        uuid
    };
    assert!(dir.join("memories.json").exists());

    let s2 = MemoryStore::open(Some(dir.clone())).unwrap();
    assert_eq!(s2.len(), 1);
    let rec = s2.get(&uuid).unwrap();
    assert_eq!(rec.content, "durable memory");
    assert_eq!(rec.prompt_id.as_deref(), Some(prompt.as_str()));

    // Witness attests across the reload boundary: chain still verifies.
    let mut wit = Witness::new();
    let out = InsertOutcome::Duplicate {
        uuid: uuid.clone(),
        observation_count: rec.observation_count + 1,
    };
    let a = wit.attest(&out, 4242);
    assert!(wit.verify_chain());
    assert_eq!(a.prev_receipt, "GENESIS");
    let link = link_prompt_memory(&prompt, &uuid);
    assert_eq!(link.prompt_uuid, prompt);
    assert_eq!(link.memory_uuid, uuid);

    let _ = std::fs::remove_dir_all(&dir);
}
