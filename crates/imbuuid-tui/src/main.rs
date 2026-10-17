// file_id: SOM-DOC-10434 (part)
// name: src/main.rs
// description: imbuuid-tui entry point — CLI (--demo/--provenance), terminal
//   setup/teardown, and the 30ms-tick event loop driving App.
// project_id: IMBUUID-MEM
// category: tui
// created: 2026-10-17
// version: 1.0.0

mod app;
mod backend;
mod capture;
mod config;
mod demo;
mod logging;
mod snapshot;
mod ui;

use app::App;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::time::Duration;

const HELP: &str = "\
imbuuid-tui — portable TUI memory browser over the imbuuid-mem store

USAGE:
    imbuuid-tui [OPTIONS]

OPTIONS:
    --demo          seed ~25 sample memories across 2 namespaces when the
                    store is empty (first-run feel)
    --provenance    print embedded build provenance (uuid, tree hash,
                    builder) and exit 0
    -h, --help      print this help

KEYS (in-app): q quit · / search · r refresh · s sort · ? help ·
               Enter detail-scroll · F5 snap · F6 font · F7 capture
";

fn print_provenance() {
    println!("provenance_uuid: {}", env!("IMBUUID_PROVENANCE_UUID"));
    println!("source_tree_hash: {}", env!("IMBUUID_SOURCE_TREE_HASH"));
    println!("build_timestamp: {}", env!("IMBUUID_BUILD_TIMESTAMP"));
    println!("builder: {}", env!("IMBUUID_BUILDER"));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--provenance") {
        print_provenance();
        return Ok(());
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{}", HELP);
        return Ok(());
    }
    let demo = args.iter().any(|a| a == "--demo");

    // Logging goes to ~/.imbuuid/tui.log — never to the terminal.
    let base_dir = backend::base_dir();
    logging::init(&base_dir).map_err(|e| format!("logging init: {}", e))?;
    std::fs::create_dir_all(&base_dir)?;

    // The v0 store opens the base DIRECTORY (snapshot = <dir>/memories.json).
    let mut store = backend::open_store(&base_dir).map_err(|e| {
        tracing::error!(error = %e, "store open failed");
        e
    })?;
    let store_path = backend::snapshot_path(&base_dir); // display only
    let mut witness = backend::new_witness();

    // Namespace label registry: the record only carries the u16 hash, so the
    // TUI remembers the labels it wrote itself (hex fallback otherwise).
    let mut ns_labels = std::collections::HashMap::new();
    for label in ["ops", "research"] {
        backend::register_label(&mut ns_labels, label);
    }

    if demo && store.is_empty() {
        match backend::seed_demo(&mut store, &mut witness, &mut ns_labels) {
            Ok(n) => tracing::info!(seeded = n, "--demo seed complete"),
            Err(e) => tracing::error!(error = %e, "--demo seed failed"),
        }
    } else if demo {
        tracing::info!(memories = store.len(), "--demo: store already populated, not seeding");
    }

    let cfg = config::load(&base_dir);
    let mut app = App::new(
        store,
        witness,
        base_dir.clone(),
        store_path,
        cfg.density,
        ns_labels,
    );

    // Terminal setup.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app);

    // Terminal teardown — always restore, even on error.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        tracing::error!(error = %e, "event loop error");
        eprintln!("imbuuid-tui error: {}", e);
    }
    tracing::info!("app exit");
    Ok(())
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // F7 capture: re-render the same frame to text after presenting.
        if app.pending_capture {
            let area = terminal.size().map(|s| {
                ratatui::layout::Rect::new(0, 0, s.width, s.height)
            })?;
            let text = ui::render_to_text(app, area);
            app.finish_capture(&text);
        }

        // 30ms poll doubles as the debounce tick quantum.
        if event::poll(Duration::from_millis(30))? {
            match event::read()? {
                Event::Key(key) => app.on_key(key),
                Event::Mouse(m) => app.on_mouse(m),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        app.tick();

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
