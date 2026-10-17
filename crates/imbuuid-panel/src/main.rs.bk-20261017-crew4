// SOM-DOC-10435 (part) — imbuuid-panel: standalone UUID-panel-canvas memory
// panel (ratatui + crossterm).
//
// Usage: imbuuid-panel [--snapshot <path>] [--provenance] [--help]
// Logging: tracing to ~/.imbuuid/panel.log (file only, never stdout).

mod provenance;
mod store;
mod ui;

use store::InMemoryStore;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

/// File-only logging: ~/.imbuuid/panel.log. Nothing goes to stdout/stderr
/// (the TUI owns the terminal).
fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = std::path::Path::new(&home).join(".imbuuid");
    let _ = std::fs::create_dir_all(&dir);
    let file_appender = tracing_appender::rolling::daily(&dir, "panel.log");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);
    let _ = tracing_subscriber::fmt()
        .with_writer(BoxMakeWriter::new(writer))
        .with_ansi(false)
        .try_init();
    tracing::info!(
        "imbuuid-panel starting (prov {})",
        provenance::PROVENANCE_UUID
    );
    guard
}

fn usage() -> &'static str {
    "imbuuid-panel — standalone UUID-panel-canvas memory panel (SOM-DOC-10435)\n\
     \n\
     Usage:\n  \
     imbuuid-panel [--snapshot <path>]   run the panel (JSON snapshot or demo seed)\n  \
     imbuuid-panel --provenance          print this binary's embedded provenance\n  \
     imbuuid-panel --help                this text\n\
     \n\
     Keys: q/Esc quit · i install funnel · ↑↓/jk select · PgUp/PgDn feed.\n\
     Log: ~/.imbuuid/panel.log (file only)."
}

fn main() {
    let _guard = init_logging(); // held for process lifetime; flushes on drop

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut snapshot: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                println!("{}", usage());
                return;
            }
            "--provenance" => {
                provenance::print_provenance();
                return;
            }
            "--snapshot" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: --snapshot needs a path\n{}", usage());
                    std::process::exit(2);
                }
                snapshot = Some(args[i].clone());
            }
            other => {
                eprintln!("error: unknown argument '{}'\n{}", other, usage());
                std::process::exit(2);
            }
        }
        i += 1;
    }

    let store = match snapshot
        .or_else(|| std::env::var("IMBUUID_PANEL_SNAPSHOT").ok())
    {
        Some(path) => match InMemoryStore::from_json_file(&path) {
            Ok(s) => {
                tracing::info!("loaded snapshot from {}", path);
                s
            }
            Err(e) => {
                tracing::error!("snapshot load failed: {}", e);
                eprintln!("error: {}", e);
                std::process::exit(2);
            }
        },
        None => {
            tracing::info!("using seeded demo store (integration seam: StoreSnapshot trait)");
            InMemoryStore::seeded()
        }
    };

    if let Err(e) = ui::run(ui::App::new(store)) {
        tracing::error!("panel error: {}", e);
        eprintln!("panel error: {}", e);
        std::process::exit(1);
    }
}
