// SOM-DOC-10435 (part) — imbuuid-rea CLI: the REA-loop provenance extractor.
//
// Usage: imbuuid-rea <path-to-binary>
//        imbuuid-rea --provenance   (print this binary's own embedded provenance)
//        imbuuid-rea --help
//
// Exit 0 with results; exit 1 with a clear message when no 0x403 UUID is
// found; exit 2 on usage / I/O errors.

mod extract;

use extract::{extract_provenance, format_utc, reconstruct_timestamp, render_hit};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Embedded provenance: compile-time constants minted by build.rs, plus the
// byte-stable record line this very extractor scans for. #[used] keeps it in
// .rodata even though Rust code never reads it — the REA loop reads it.
// ---------------------------------------------------------------------------
const PROVENANCE_UUID: &str = env!("IMBUUID_PROVENANCE_UUID");
const SOURCE_TREE_HASH: &str = env!("IMBUUID_SOURCE_TREE_HASH");
const BUILD_TIMESTAMP: &str = env!("IMBUUID_BUILD_TIMESTAMP");
const BUILDER: &str = env!("IMBUUID_BUILDER");

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

fn print_own_provenance() {
    // Surface through the imbuuid-mem crate API (canonical path) …
    println!("imbuuid-rea binary provenance (via imbuuid_mem::provenance::binary_provenance):");
    match imbuuid_mem::provenance::binary_provenance() {
        Some(bp) => println!("{:#?}", bp),
        None => println!("<none: build script did not emit provenance env>"),
    }
    // … and the raw embedded constants (what the REA extractor itself reads).
    println!("embedded constants:");
    println!("  IMBUUID_PROVENANCE_UUID  = {}", PROVENANCE_UUID);
    println!("  IMBUUID_SOURCE_TREE_HASH = {}", SOURCE_TREE_HASH);
    println!("  IMBUUID_BUILD_TIMESTAMP  = {} ({})", BUILD_TIMESTAMP, format_utc(BUILD_TIMESTAMP.parse().unwrap_or(0)));
    println!("  IMBUUID_BUILDER          = {}", BUILDER);
    println!("  record line              = {}", PROVENANCE_RECORD);
}

fn usage() -> String {
    format!(
        "imbuuid-rea — REA-loop binary provenance extractor (SOM-DOC-10435)\n\
         \n\
         Usage:\n  \
         imbuuid-rea <path-to-binary>   scan bytes for UUIDv8 TYPE_BINARY_PROVENANCE (0x403)\n  \
         imbuuid-rea --provenance        print this binary's own embedded provenance\n  \
         imbuuid-rea --help              this text\n\
         \n\
         The scan regex bakes in the UUIDv8 version nibble (8) and RFC 9562\n\
         variant bits ([89ab]): extraction simultaneously verifies compliance.\n\
         Exit 0 with results, 1 when no provenance UUID is found, 2 on errors."
    )
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = match args.as_slice() {
        [a] => a.as_str(),
        _ => {
            eprintln!("{}", usage());
            eprintln!("\nerror: expected exactly one argument (path, --provenance, or --help)");
            return ExitCode::from(2);
        }
    };

    match arg {
        "--help" | "-h" => {
            println!("{}", usage());
            return ExitCode::SUCCESS;
        }
        "--provenance" => {
            print_own_provenance();
            return ExitCode::SUCCESS;
        }
        path => {
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("error: cannot read '{}': {}", path, e);
                    return ExitCode::from(2);
                }
            };
            let hits = extract_provenance(&bytes);
            if hits.is_empty() {
                eprintln!(
                    "no provenance found: '{}' ({} bytes) contains no UUIDv8 \
                     TYPE_BINARY_PROVENANCE (0x403) record",
                    path,
                    bytes.len()
                );
                return ExitCode::from(1);
            }
            let now = now_sec();
            println!(
                "REA extraction: {} provenance hit(s) in '{}' ({} bytes)\n",
                hits.len(),
                path,
                bytes.len()
            );
            for (i, hit) in hits.iter().enumerate() {
                if i > 0 {
                    println!("---");
                }
                print!("{}", render_hit(hit, now));
                // Cross-check: the embedded record timestamp vs the 24-bit reconstruction.
                if let Some(ts) = hit.record_timestamp {
                    let recon = reconstruct_timestamp(hit.decoded.timestamp_sec, now);
                    if ts == recon || (ts as i64 - recon as i64).abs() < 2 * 86400 {
                        println!("  consistency     : record ts and UUID field agree");
                    } else {
                        println!(
                            "  consistency     : NOTE record ts {} != reconstructed {}",
                            format_utc(ts),
                            format_utc(recon)
                        );
                    }
                }
            }
            ExitCode::SUCCESS
        }
    }
}
