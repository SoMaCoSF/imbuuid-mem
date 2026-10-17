// SOM-DOC-10435 (part) — ratatui panel: tile grid, live feed, install funnel.
//
// HEX-PANEL INTERPRETATION (honest terminal edition): true hexagons cannot
// tile a character grid, so the UUID panel-canvas aesthetic is interpreted
// as — (1) every tile is a hex cell, marked by ⬡ glyphs in titles and
// honeycomb divider strips (⬡⬢⬡⬢) between rows; (2) double-line borders
// carry the hex edge weight; (3) gold #C9A227 is reserved for the
// live/active/selected element (estate brand law); (4) the header is a dense
// mono KPI strip in the Activation-Command aesthetic. See
// docs/PANEL-AND-PROVENANCE.md.

use crate::provenance;
use crate::store::{InMemoryStore, StoreSnapshot};
use chrono::{TimeZone, Utc};
use crossterm::event::{self, Event, KeyCode, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::io;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The one and only outbound URL. This panel NEVER downloads or installs:
/// activation opens this link in the user's browser (founder-tapped funnel).
pub const GUMROAD_URL: &str = "https://somacosf.gumroad.com/l/imbue";

const GOLD: Color = Color::Rgb(201, 162, 39);
const INK: Color = Color::Rgb(8, 10, 14);
const PANEL_BG: Color = Color::Rgb(13, 17, 23);
const DIM: Color = Color::Rgb(120, 130, 145);
const CYAN: Color = Color::Rgb(86, 182, 194);
const RED: Color = Color::Rgb(200, 70, 70);
const GREEN: Color = Color::Rgb(110, 180, 110);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Normal,
    InstallDialog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogChoice {
    OpenLink,
    Cancel,
}

pub struct App {
    pub store: InMemoryStore,
    pub selected: usize,
    pub feed_scroll: usize,
    pub mode: Mode,
    pub dialog_choice: DialogChoice,
    pub dialog_status: String,
    pub install_rect: Rect,
    pub dialog_open_rect: Rect,
    pub dialog_cancel_rect: Rect,
    last_tick: Instant,
}

impl App {
    pub fn new(store: InMemoryStore) -> Self {
        App {
            store,
            selected: 0,
            feed_scroll: 0,
            mode: Mode::Normal,
            dialog_choice: DialogChoice::OpenLink,
            dialog_status: String::new(),
            install_rect: Rect::default(),
            dialog_open_rect: Rect::default(),
            dialog_cancel_rect: Rect::default(),
            last_tick: Instant::now(),
        }
    }

    fn open_install_dialog(&mut self) {
        self.mode = Mode::InstallDialog;
        self.dialog_choice = DialogChoice::OpenLink;
        self.dialog_status.clear();
        tracing::info!("install dialog opened (funnel, no download)");
    }

    fn close_dialog(&mut self) {
        self.mode = Mode::Normal;
        self.dialog_status.clear();
    }

    /// Confirm: the ONLY outbound action — open the Gumroad URL in the
    /// browser. Never downloads, never installs, never shells out beyond
    /// the `open` crate. Failures print the URL instead of failing silent.
    fn confirm_install(&mut self) {
        tracing::info!("install funnel confirmed — opening {}", GUMROAD_URL);
        match open::that(GUMROAD_URL) {
            Ok(()) => {
                self.dialog_status = format!("browser opened → {}", GUMROAD_URL);
                tracing::info!("browser opened for {}", GUMROAD_URL);
            }
            Err(e) => {
                self.dialog_status = format!("could not open browser ({}); URL: {}", e, GUMROAD_URL);
                tracing::warn!("open::that failed: {}", e);
            }
        }
    }

    fn on_key(&mut self, code: KeyCode) -> bool {
        // Returns true when the app should quit.
        match self.mode {
            Mode::InstallDialog => match code {
                KeyCode::Esc => {
                    tracing::info!("install dialog cancelled (Esc)");
                    self.close_dialog();
                }
                KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
                    self.dialog_choice = match self.dialog_choice {
                        DialogChoice::OpenLink => DialogChoice::Cancel,
                        DialogChoice::Cancel => DialogChoice::OpenLink,
                    };
                }
                KeyCode::Enter => match self.dialog_choice {
                    DialogChoice::OpenLink => self.confirm_install(),
                    DialogChoice::Cancel => {
                        tracing::info!("install dialog cancelled (button)");
                        self.close_dialog();
                    }
                },
                _ => {}
            },
            Mode::Normal => match code {
                KeyCode::Char('q') | KeyCode::Esc => return true,
                KeyCode::Char('i') | KeyCode::Char('I') => self.open_install_dialog(),
                KeyCode::Down | KeyCode::Char('j') => {
                    let n = self.store.memories().len();
                    if n > 0 {
                        self.selected = (self.selected + 1) % n;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let n = self.store.memories().len();
                    if n > 0 {
                        self.selected = (self.selected + n - 1) % n;
                    }
                }
                KeyCode::PageDown => self.feed_scroll = self.feed_scroll.saturating_add(5),
                KeyCode::PageUp => self.feed_scroll = self.feed_scroll.saturating_sub(5),
                _ => {}
            },
        }
        false
    }

    fn on_click(&mut self, column: u16, row: u16) {
        let inside = |r: Rect| {
            column >= r.x && column < r.x + r.width && row >= r.y && row < r.y + r.height
        };
        match self.mode {
            Mode::InstallDialog => {
                if inside(self.dialog_open_rect) {
                    self.dialog_choice = DialogChoice::OpenLink;
                    self.confirm_install();
                } else if inside(self.dialog_cancel_rect) {
                    tracing::info!("install dialog cancelled (mouse)");
                    self.close_dialog();
                }
            }
            Mode::Normal => {
                if inside(self.install_rect) {
                    self.open_install_dialog();
                }
            }
        }
    }

    fn tick(&mut self) {
        // Simulated live witness heartbeat (documented; seam for real stream).
        if self.last_tick.elapsed() >= Duration::from_secs(3) {
            self.store.tick();
            self.last_tick = Instant::now();
        }
    }
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn fmt_ts(ts: u64) -> String {
    Utc.timestamp_opt(ts as i64, 0)
        .single()
        .map(|d| d.format("%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn honeycomb(width: usize) -> String {
    "⬡⬢".repeat(width / 2 + 1)[..width.min(400)].to_string()
}

fn hex_block(title: &str, accent: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(symbols::border::DOUBLE)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(PANEL_BG))
        .title(format!(" ⬡ {} ", title))
        .title_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
}

/// Render one full frame. Pure function of `&mut App` (rects are recorded
/// for mouse hit-testing); also used by the TestBackend unit tests.
pub fn render_frame(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(INK)), area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header KPI strip
            Constraint::Length(1), // honeycomb divider
            Constraint::Length(9), // tile row: stats | namespaces | install
            Constraint::Min(8),    // feed
            Constraint::Length(12),// memories | detail
            Constraint::Length(1), // footer
        ])
        .split(area);

    render_header(f, rows[0]);
    f.render_widget(
        Paragraph::new(honeycomb(rows[1].width as usize))
            .style(Style::default().fg(DIM).bg(INK)),
        rows[1],
    );

    // --- tile row ---
    let tiles = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[2]);
    render_stats(f, tiles[0], &app.store);
    render_namespaces(f, tiles[1], &app.store);
    app.install_rect = tiles[2];
    render_install_tile(f, tiles[2]);

    // --- attestation feed ---
    render_feed(f, rows[3], app);

    // --- memories | detail ---
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(rows[4]);
    render_memory_list(f, bottom[0], app);
    render_detail(f, bottom[1], app);

    // --- footer ---
    let demo_tag = if app.store.demo { "DEMO DATA" } else { "SNAPSHOT" };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" q", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(" quit   ", Style::default().fg(DIM)),
            Span::styled("i", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(" install imbue   ", Style::default().fg(DIM)),
            Span::styled("↑↓/jk", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(" select memory   ", Style::default().fg(DIM)),
            Span::styled("PgUp/PgDn", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(" scroll feed   ", Style::default().fg(DIM)),
            Span::styled(demo_tag, Style::default().fg(RED).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(INK)),
        rows[5],
    );

    if app.mode == Mode::InstallDialog {
        render_install_dialog(f, area, app);
    }
}

fn render_header(f: &mut Frame, area: Rect) {
    let prov = provenance::provenance_short();
    let title = Line::from(vec![
        Span::styled(" ⬡ IMBUUID ", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
        Span::styled("MEMORY PANEL", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("   prov {}   builder {}   {}", prov, provenance::BUILDER, fmt_ts(now_sec())),
            Style::default().fg(DIM),
        ),
    ]);
    f.render_widget(
        Paragraph::new(title)
            .block(hex_block("UUID PANEL CANVAS", GOLD))
            .alignment(Alignment::Left),
        area,
    );
}

fn render_stats(f: &mut Frame, area: Rect, store: &dyn StoreSnapshot) {
    let s = store.stats();
    let lines = vec![
        Line::from(vec![
            Span::styled("memories      ", Style::default().fg(DIM)),
            Span::styled(format!("{}", s.total_memories), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("attestations  ", Style::default().fg(DIM)),
            Span::styled(format!("{}", s.total_attestations), Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("dedup collapse", Style::default().fg(DIM)),
            Span::styled(format!("s  {}", s.dedup_collapses), Style::default().fg(GREEN)),
        ]),
        Line::from(vec![
            Span::styled("namespaces    ", Style::default().fg(DIM)),
            Span::styled(format!("{}", s.namespaces.len()), Style::default().fg(Color::White)),
        ]),
        Line::from(Span::styled(
            "content-hash dedup · witness plane live",
            Style::default().fg(DIM),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).block(hex_block("STORE STATS", CYAN)),
        area,
    );
}

fn render_namespaces(f: &mut Frame, area: Rect, store: &dyn StoreSnapshot) {
    let s = store.stats();
    let items: Vec<ListItem> = s
        .namespaces
        .iter()
        .map(|(ns, n)| {
            ListItem::new(Line::from(vec![
                Span::styled("⬢ ", Style::default().fg(GOLD)),
                Span::styled(ns.clone(), Style::default().fg(Color::White)),
                Span::styled(format!("  {}", n), Style::default().fg(DIM)),
            ]))
        })
        .collect();
    f.render_widget(
        List::new(items).block(hex_block("NAMESPACES", CYAN)),
        area,
    );
}

fn render_install_tile(f: &mut Frame, area: Rect) {
    let body = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  INSTALL IMBUE",
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "  Desktop / Portable",
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  press ",
            Style::default().fg(DIM),
        ), Span::styled("i", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)), Span::styled(" or click", Style::default().fg(DIM))]),
        Line::from(vec![Span::styled(
            "  funnel only — never installs",
            Style::default().fg(DIM),
        )]),
    ];
    f.render_widget(
        Paragraph::new(body)
            .block(hex_block("GET IMBUE", GOLD))
            .alignment(Alignment::Left),
        area,
    );
}

fn render_feed(f: &mut Frame, area: Rect, app: &App) {
    let feed = app.store.feed(200);
    let total = feed.len();
    let start = app.feed_scroll.min(total.saturating_sub(1));
    let items: Vec<ListItem> = feed
        .iter()
        .skip(start)
        .take(area.height.saturating_sub(2) as usize)
        .map(|a| {
            let verdict_style = if a.verdict == "ATTESTED" {
                Style::default().fg(GREEN)
            } else {
                Style::default().fg(RED).add_modifier(Modifier::BOLD)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("#{} ", a.seq), Style::default().fg(DIM)),
                Span::styled(a.witness.clone(), Style::default().fg(CYAN)),
                Span::styled(" → ", Style::default().fg(DIM)),
                Span::styled(format!("{:.8}…", &a.memory_uuid[..8.min(a.memory_uuid.len())]), Style::default().fg(Color::White)),
                Span::styled("  ", Style::default()),
                Span::styled(a.verdict.clone(), verdict_style),
                Span::styled(format!("  {}", fmt_ts(a.ts_sec)), Style::default().fg(DIM)),
            ]))
        })
        .collect();
    f.render_widget(
        List::new(items).block(hex_block(
            &format!("WITNESS ATTESTATIONS — LIVE ({} shown)", total - start),
            GREEN,
        )),
        area,
    );
}

fn render_memory_list(f: &mut Frame, area: Rect, app: &mut App) {
    let items: Vec<ListItem> = app
        .store
        .memories()
        .iter()
        .map(|m| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:.8}…", &m.uuid[..8.min(m.uuid.len())]), Style::default().fg(Color::White)),
                Span::styled(format!(" {}", m.namespace), Style::default().fg(DIM)),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    if !app.store.memories().is_empty() {
        state.select(Some(app.selected.min(app.store.memories().len() - 1)));
    }
    f.render_stateful_widget(
        List::new(items)
            .block(hex_block("MEMORIES", CYAN))
            .highlight_style(Style::default().bg(GOLD).fg(INK).add_modifier(Modifier::BOLD))
            .highlight_symbol("⬡ "),
        area,
        &mut state,
    );
}

fn render_detail(f: &mut Frame, area: Rect, app: &App) {
    let block = hex_block("MEMORY DETAIL", GOLD);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(m) = app.store.memories().get(app.selected) else {
        f.render_widget(Paragraph::new("no memories"), inner);
        return;
    };
    let rows = vec![
        ("uuid", m.uuid.clone(), Color::White),
        ("namespace", m.namespace.clone(), CYAN),
        ("signal", format!("{:.4}", m.signal), GREEN),
        ("attestations", format!("{}", m.attestations), Color::White),
        ("created", fmt_ts(m.created_sec), DIM),
        ("content-hash", format!("{:.16}…", m.content_hash), DIM),
    ];
    let mut lines: Vec<Line> = rows
        .into_iter()
        .map(|(k, v, c)| {
            Line::from(vec![
                Span::styled(format!("{:<13}", k), Style::default().fg(DIM)),
                Span::styled(v, Style::default().fg(c)),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("summary", Style::default().fg(DIM))));
    lines.push(Line::from(Span::styled(m.summary.clone(), Style::default().fg(Color::White))));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup[1])[1]
}

fn render_install_dialog(f: &mut Frame, area: Rect, app: &mut App) {
    let dlg = centered_rect(70, 60, area);
    f.render_widget(Clear, dlg);
    let block = hex_block("INSTALL IMBUE DESKTOP / PORTABLE", GOLD);
    let inner = block.inner(dlg);
    f.render_widget(block, dlg);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),   // body
            Constraint::Length(3),// buttons
            Constraint::Length(2),// status
        ])
        .split(inner);

    let body = vec![
        Line::from("Get the full Imbue build (Windows .exe / portable):"),
        Line::from(vec![Span::styled(
            GUMROAD_URL,
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD).add_modifier(Modifier::UNDERLINED),
        )]),
        Line::from(""),
        Line::from("  1. Open the link in your browser (button below does this)."),
        Line::from("  2. Complete checkout on Gumroad."),
        Line::from(vec![Span::styled(
            "  3. Installs are founder-tapped: this panel NEVER downloads",
            Style::default().fg(DIM),
        )]),
        Line::from(vec![Span::styled(
            "     or installs anything silently — it only opens the link.",
            Style::default().fg(DIM),
        )]),
    ];
    f.render_widget(Paragraph::new(body).wrap(Wrap { trim: true }), chunks[0]);

    let btns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[1]);
    let open_style = if app.dialog_choice == DialogChoice::OpenLink {
        Style::default().bg(GOLD).fg(INK).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DIM)
    };
    let cancel_style = if app.dialog_choice == DialogChoice::Cancel {
        Style::default().bg(GOLD).fg(INK).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DIM)
    };
    app.dialog_open_rect = btns[0];
    app.dialog_cancel_rect = btns[1];
    f.render_widget(
        Paragraph::new("  [ Open Gumroad link ]").style(open_style).alignment(Alignment::Center),
        btns[0],
    );
    f.render_widget(
        Paragraph::new("[ Cancel ]  (Esc)").style(cancel_style).alignment(Alignment::Center),
        btns[1],
    );

    let status_style = if app.dialog_status.contains("could not") {
        Style::default().fg(RED)
    } else {
        Style::default().fg(GREEN)
    };
    f.render_widget(
        Paragraph::new(app.dialog_status.clone()).style(status_style).wrap(Wrap { trim: true }),
        chunks[2],
    );
}

/// Run the interactive panel. Returns on quit.
pub fn run(mut app: App) -> io::Result<()> {
    use crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    tracing::info!("panel started");

    let result = (|| -> io::Result<()> {
        loop {
            app.tick();
            terminal.draw(|f| render_frame(f, &mut app))?;
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(key) => {
                        if app.on_key(key.code) {
                            return Ok(());
                        }
                    }
                    Event::Mouse(m) => {
                        if m.kind == MouseEventKind::Down(MouseButton::Left) {
                            app.on_click(m.column, m.row);
                        }
                    }
                    _ => {}
                }
            }
        }
    })();

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    tracing::info!("panel exited");
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn test_app() -> App {
        App::new(InMemoryStore::seeded())
    }

    fn render_to_string(app: &mut App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render_frame(f, app)).unwrap();
        let mut s = String::new();
        for y in 0..h {
            for x in 0..w {
                s.push(terminal.backend().buffer()[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn frame_contains_all_tiles_and_funnel() {
        let mut app = test_app();
        let out = render_to_string(&mut app, 120, 40);
        for needle in [
            "IMBUUID",
            "MEMORY PANEL",
            "STORE STATS",
            "NAMESPACES",
            "INSTALL IMBUE",
            "WITNESS ATTESTATIONS",
            "MEMORIES",
            "MEMORY DETAIL",
            "agent:planner",
            "ATTESTED",
        ] {
            assert!(out.contains(needle), "frame missing {:?}", needle);
        }
        // Stats numbers from the seeded store.
        assert!(out.contains("12")); // memories
        assert!(out.contains("40")); // attestations
    }

    #[test]
    fn install_dialog_opens_and_shows_url() {
        let mut app = test_app();
        assert_eq!(app.mode, Mode::Normal);
        app.on_key(KeyCode::Char('i'));
        assert_eq!(app.mode, Mode::InstallDialog);
        let out = render_to_string(&mut app, 120, 40);
        assert!(out.contains(GUMROAD_URL));
        assert!(out.contains("founder-tapped"));
        assert!(out.contains("NEVER downloads"));
        // Esc cancels.
        app.on_key(KeyCode::Esc);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn memory_navigation_wraps() {
        let mut app = test_app();
        let n = app.store.memories().len();
        app.on_key(KeyCode::Up);
        assert_eq!(app.selected, n - 1);
        app.on_key(KeyCode::Down);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn clicking_install_tile_opens_dialog() {
        let mut app = test_app();
        let _ = render_to_string(&mut app, 120, 40); // records install_rect
        let r = app.install_rect;
        assert!(r.width > 0, "install rect recorded");
        app.on_click(r.x + 2, r.y + 2);
        assert_eq!(app.mode, Mode::InstallDialog);
    }

    #[test]
    fn gumroad_url_is_exact() {
        assert_eq!(GUMROAD_URL, "https://somacosf.gumroad.com/l/imbue");
    }
}
