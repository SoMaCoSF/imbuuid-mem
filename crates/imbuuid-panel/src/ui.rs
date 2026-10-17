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

use crate::browser::{self, Resolution};
use crate::dossier;
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
use std::env;
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
    Browser,
    Dossier,
    Publish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogChoice {
    OpenLink,
    Cancel,
}

/// UUID browser source tabs: store | dossier file | paste.
pub const BROWSER_SOURCES: [&str; 3] = ["store", "dossier-file", "paste"];

/// Which single-line editor the browser has open (paste UUID / dossier path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserEdit {
    Paste,
    DossierPath,
}

/// Browser view state (used when Mode::Browser).
pub struct BrowserUi {
    pub source: usize,
    pub dossier_path: String,
    pub dossier_uuids: Vec<String>,
    pub edit: Option<BrowserEdit>,
    pub edit_buffer: String,
    /// Last confirmed paste input (kept so the paste tab isn't empty).
    pub last_paste: String,
    pub selected: usize,
    pub detail: Option<Resolution>,
    pub status: String,
}

impl BrowserUi {
    fn new() -> Self {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        BrowserUi {
            source: 0,
            dossier_path: format!("{}/.imbuuid/dossier.md", home),
            dossier_uuids: Vec::new(),
            edit: None,
            edit_buffer: String::new(),
            last_paste: String::new(),
            selected: 0,
            detail: None,
            status: String::new(),
        }
    }

    fn reset(&mut self) {
        self.source = 0;
        self.edit = None;
        self.edit_buffer.clear();
        self.last_paste.clear();
        self.selected = 0;
        self.detail = None;
        self.status.clear();
    }

    /// The UUID list for the active source.
    fn current_uuids(&self, store: &dyn StoreSnapshot) -> Vec<String> {
        match self.source {
            0 => browser::store_uuids(store),
            1 => self.dossier_uuids.clone(),
            // Paste tab: the last confirmed paste input.
            _ => {
                if self.last_paste.len() >= 32 {
                    vec![self.last_paste.clone()]
                } else {
                    Vec::new()
                }
            }
        }
    }
}

/// Dossier preview state (used when Mode::Dossier).
pub struct DossierUi {
    pub slug: String,
    pub payload: String,
    pub scroll: usize,
}

/// Publish dialog state (used when Mode::Publish). Choice defaults to
/// Cancel — publishing is never the accidental option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishChoice {
    Gist,
    Repo,
    Cancel,
}

pub struct PublishUi {
    pub choice: PublishChoice,
    pub status: String,
    pub gist_rect: Rect,
    pub repo_rect: Rect,
    pub cancel_rect: Rect,
}

impl PublishUi {
    fn new() -> Self {
        PublishUi {
            choice: PublishChoice::Cancel,
            status: String::new(),
            gist_rect: Rect::default(),
            repo_rect: Rect::default(),
            cancel_rect: Rect::default(),
        }
    }
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
    pub browser: BrowserUi,
    pub dossier: Option<DossierUi>,
    pub publish: PublishUi,
    pub browser_tile_rect: Rect,
    pub dossier_tile_rect: Rect,
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
            browser: BrowserUi::new(),
            dossier: None,
            publish: PublishUi::new(),
            browser_tile_rect: Rect::default(),
            dossier_tile_rect: Rect::default(),
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

    /// Open the UUID browser (tile / `b`). The UUID is the address; the
    /// browser decodes and resolves — never invents.
    fn open_browser(&mut self) {
        self.browser.reset();
        self.mode = Mode::Browser;
        tracing::info!("uuid browser opened");
    }

    /// Compile the contextual dossier and open the in-panel preview
    /// (SOM-DOC-10439). Nothing leaves the machine from here.
    fn open_dossier(&mut self) {
        let draft = dossier::compile_dossier(&self.store, "panel scope");
        let payload = dossier::render_markdown(&draft);
        tracing::info!(
            entries = draft.entries.len(),
            bytes = payload.len(),
            "dossier compiled, preview opened"
        );
        self.dossier = Some(DossierUi {
            slug: draft.slug,
            payload,
            scroll: 0,
        });
        self.mode = Mode::Dossier;
    }

    /// Load the dossier-file source for the browser: open a dossier `.md`,
    /// extract exactly the UUIDs it carries.
    fn browser_load_dossier_file(&mut self) {
        let path = self.browser.dossier_path.trim().to_string();
        match browser::load_dossier_uuids(&path) {
            Ok(uuids) => {
                self.browser.status =
                    format!("{} UUIDs extracted from {}", uuids.len(), path);
                tracing::info!(path = %path, uuids = uuids.len(), "browser.dossier_loaded");
                self.browser.dossier_uuids = uuids;
                self.browser.selected = 0;
                self.browser.detail = None;
            }
            Err(e) => {
                self.browser.status = e.clone();
                tracing::warn!("browser dossier load failed: {}", e);
            }
        }
    }

    /// Resolve the currently selected UUID in the active browser source.
    fn browser_resolve_selected(&mut self) {
        let uuids = self.browser.current_uuids(&self.store);
        let Some(u) = uuids.get(self.browser.selected).cloned() else {
            self.browser.status = "nothing to resolve — list is empty".to_string();
            return;
        };
        tracing::info!(uuid = %u, "browser.resolve_selected");
        self.browser.detail = Some(browser::resolve(&u, &self.store));
    }

    /// Confirm the publish dialog choice. Gist posts ONLY via `gh gist
    /// create` after this explicit confirm; every failure path lands in the
    /// outbox with an exact report. Never posts silently.
    fn confirm_publish(&mut self) {
        let Some(d) = &self.dossier else {
            self.publish.status = "no dossier draft — nothing to publish".to_string();
            return;
        };
        let slug = d.slug.clone();
        let payload = d.payload.clone();
        match self.publish.choice {
            PublishChoice::Cancel => {
                tracing::info!("dossier publish cancelled");
                self.mode = Mode::Dossier;
            }
            PublishChoice::Gist => {
                let report = dossier::publish_gist_or_outbox(&slug, &payload);
                self.publish.status = report.describe();
                tracing::info!("dossier publish choice=gist: {}", self.publish.status);
            }
            PublishChoice::Repo => {
                let root =
                    env::var("IMBUUID_DOSSIER_REPO").unwrap_or_else(|_| ".".to_string());
                match dossier::publish_to_repo(&root, &slug, &payload) {
                    Ok(report) => {
                        self.publish.status = report.describe();
                        tracing::info!("dossier publish choice=repo: {}", self.publish.status);
                    }
                    Err(e) => {
                        self.publish.status = format!("repo write failed: {}", e);
                        tracing::warn!("dossier publish choice=repo failed: {}", e);
                    }
                }
            }
        }
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
                KeyCode::Char('b') | KeyCode::Char('B') => self.open_browser(),
                KeyCode::Char('d') | KeyCode::Char('D') => self.open_dossier(),
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
            Mode::Browser => return self.on_key_browser(code),
            Mode::Dossier => return self.on_key_dossier(code),
            Mode::Publish => return self.on_key_publish(code),
        }
        false
    }

    /// Key handling for the UUID browser. Returns true when the app quits.
    fn on_key_browser(&mut self, code: KeyCode) -> bool {
        // Single-line editors first (paste UUID / dossier path).
        if let Some(target) = self.browser.edit {
            match code {
                KeyCode::Esc => {
                    self.browser.edit = None;
                    self.browser.edit_buffer.clear();
                    self.browser.status = "edit cancelled".to_string();
                }
                KeyCode::Enter => {
                    let buf = self.browser.edit_buffer.trim().to_string();
                    self.browser.edit = None;
                    self.browser.edit_buffer.clear();
                    match target {
                        BrowserEdit::Paste => {
                            self.browser.last_paste = buf.clone();
                            self.browser.source = 2;
                            self.browser.selected = 0;
                            self.browser.detail = Some(browser::resolve(&buf, &self.store));
                            self.browser.status = "pasted input decoded".to_string();
                            tracing::info!("browser: paste resolved");
                        }
                        BrowserEdit::DossierPath => {
                            if !buf.is_empty() {
                                self.browser.dossier_path = buf;
                            }
                            self.browser_load_dossier_file();
                        }
                    }
                }
                KeyCode::Backspace => {
                    self.browser.edit_buffer.pop();
                }
                KeyCode::Char(c) => {
                    if self.browser.edit_buffer.len() < 256 {
                        self.browser.edit_buffer.push(c);
                    }
                }
                _ => {}
            }
            return false;
        }
        let n = self.browser.current_uuids(&self.store).len();
        match code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Tab => {
                self.browser.source = (self.browser.source + 1) % BROWSER_SOURCES.len();
                self.browser.selected = 0;
                self.browser.detail = None;
                self.browser.status.clear();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if n > 0 {
                    self.browser.selected = (self.browser.selected + 1) % n;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if n > 0 {
                    self.browser.selected = (self.browser.selected + n - 1) % n;
                }
            }
            KeyCode::Enter => self.browser_resolve_selected(),
            // `p`: paste a UUID manually. `o`: type a dossier path, Enter loads.
            KeyCode::Char('p') => {
                self.browser.edit = Some(BrowserEdit::Paste);
                self.browser.edit_buffer.clear();
                self.browser.status = "paste a UUIDv8, Enter decodes".to_string();
            }
            KeyCode::Char('o') => {
                self.browser.edit = Some(BrowserEdit::DossierPath);
                self.browser.edit_buffer = self.browser.dossier_path.clone();
                self.browser.status = "type a dossier .md path, Enter loads".to_string();
            }
            _ => {}
        }
        false
    }

    /// Key handling for the dossier preview. Returns true when the app quits.
    fn on_key_dossier(&mut self, code: KeyCode) -> bool {
        let lines = self
            .dossier
            .as_ref()
            .map(|d| d.payload.lines().count())
            .unwrap_or(0);
        match code {
            KeyCode::Esc => self.mode = Mode::Normal,
            // `p`: open the explicit-choice PUBLISH dialog.
            KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Enter => {
                self.publish = PublishUi::new();
                self.mode = Mode::Publish;
                tracing::info!("dossier publish dialog opened");
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(d) = self.dossier.as_mut() {
                    d.scroll = (d.scroll + 1).min(lines.saturating_sub(1));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(d) = self.dossier.as_mut() {
                    d.scroll = d.scroll.saturating_sub(1);
                }
            }
            KeyCode::PageDown => {
                if let Some(d) = self.dossier.as_mut() {
                    d.scroll = (d.scroll + 10).min(lines.saturating_sub(1));
                }
            }
            KeyCode::PageUp => {
                if let Some(d) = self.dossier.as_mut() {
                    d.scroll = d.scroll.saturating_sub(10);
                }
            }
            _ => {}
        }
        false
    }

    /// Key handling for the publish dialog. Returns true when the app quits.
    /// Default choice is Cancel; publishing takes an explicit Enter.
    fn on_key_publish(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Esc => self.mode = Mode::Dossier,
            KeyCode::Tab | KeyCode::Right | KeyCode::Left | KeyCode::Up | KeyCode::Down => {
                self.publish.choice = match self.publish.choice {
                    PublishChoice::Gist => PublishChoice::Repo,
                    PublishChoice::Repo => PublishChoice::Cancel,
                    PublishChoice::Cancel => PublishChoice::Gist,
                };
            }
            KeyCode::Enter => self.confirm_publish(),
            _ => {}
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
                } else if inside(self.browser_tile_rect) {
                    tracing::info!("uuid browser opened (mouse)");
                    self.open_browser();
                } else if inside(self.dossier_tile_rect) {
                    tracing::info!("dossier preview opened (mouse)");
                    self.open_dossier();
                }
            }
            Mode::Publish => {
                if inside(self.publish.gist_rect) {
                    self.publish.choice = PublishChoice::Gist;
                    self.confirm_publish();
                } else if inside(self.publish.repo_rect) {
                    self.publish.choice = PublishChoice::Repo;
                    self.confirm_publish();
                } else if inside(self.publish.cancel_rect) {
                    tracing::info!("dossier publish cancelled (mouse)");
                    self.mode = Mode::Dossier;
                }
            }
            _ => {}
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
    // Char-wise (not byte-sliced): the ⬡/⬢ glyphs are 3 bytes each, so
    // byte indexing panics at widths that aren't multiples of 6.
    "⬡⬢".chars().cycle().take(width.min(400)).collect()
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
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(rows[2]);
    render_stats(f, tiles[0], &app.store);
    render_namespaces(f, tiles[1], &app.store);
    app.browser_tile_rect = tiles[2];
    render_browser_tile(f, tiles[2]);
    app.dossier_tile_rect = tiles[3];
    render_dossier_tile(f, tiles[3]);
    app.install_rect = tiles[4];
    render_install_tile(f, tiles[4]);

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
            Span::styled("b", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(" browser   ", Style::default().fg(DIM)),
            Span::styled("d", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(" dossier   ", Style::default().fg(DIM)),
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
    match app.mode {
        Mode::Browser => render_browser(f, area, app),
        Mode::Dossier => render_dossier(f, area, app),
        Mode::Publish => render_publish(f, area, app),
        _ => {}
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

fn render_browser_tile(f: &mut Frame, area: Rect) {
    let body = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  UUID BROWSER",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "  decode → resolve",
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  press ",
            Style::default().fg(DIM),
        ), Span::styled("b", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)), Span::styled(" or click", Style::default().fg(DIM))]),
        Line::from(vec![Span::styled(
            "  UUID = the address",
            Style::default().fg(DIM),
        )]),
    ];
    f.render_widget(
        Paragraph::new(body)
            .block(hex_block("BROWSE", CYAN))
            .alignment(Alignment::Left),
        area,
    );
}

fn render_dossier_tile(f: &mut Frame, area: Rect) {
    let body = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  DOSSIER",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "  compile · preview",
            Style::default().fg(Color::White),
        )]),
        Line::from(vec![Span::styled(
            "  publish",
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  press ",
            Style::default().fg(DIM),
        ), Span::styled("d", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)), Span::styled(" or click", Style::default().fg(DIM))]),
        Line::from(vec![Span::styled(
            "  receipt-backed, never invented",
            Style::default().fg(DIM),
        )]),
    ];
    f.render_widget(
        Paragraph::new(body)
            .block(hex_block("DOSSIER", CYAN))
            .alignment(Alignment::Left),
        area,
    );
}

/// UUID browser overlay: source tabs | UUID list | decode→resolve detail.
fn render_browser(f: &mut Frame, area: Rect, app: &mut App) {
    let dlg = centered_rect(92, 88, area);
    f.render_widget(Clear, dlg);
    let block = hex_block("UUID BROWSER — DECODE → RESOLVE", CYAN);
    let inner = block.inner(dlg);
    f.render_widget(block, dlg);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // source tabs
            Constraint::Min(6),    // list | detail
            Constraint::Length(3), // status + hints
        ])
        .split(inner);

    // --- source tabs ---
    let mut tab_spans = Vec::new();
    for (i, name) in BROWSER_SOURCES.iter().enumerate() {
        let style = if app.browser.source == i {
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(DIM)
        };
        tab_spans.push(Span::styled(format!(" [{}] ", name), style));
    }
    let mut tab_lines = vec![Line::from(tab_spans)];
    if app.browser.source == 1 {
        tab_lines.push(Line::from(vec![
            Span::styled("  dossier: ", Style::default().fg(DIM)),
            Span::styled(app.browser.dossier_path.clone(), Style::default().fg(CYAN)),
            Span::styled("  (o: type path, Enter loads)", Style::default().fg(DIM)),
        ]));
    }
    f.render_widget(Paragraph::new(tab_lines), chunks[0]);

    // --- list | detail ---
    let uuids = app.browser.current_uuids(&app.store);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(chunks[1]);

    let items: Vec<ListItem> = uuids
        .iter()
        .map(|u| {
            let label = browser::decode_view(u)
                .map(|v| v.type_label)
                .unwrap_or_else(|_| "?".to_string());
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:.8}…", u), Style::default().fg(Color::White)),
                Span::styled(format!("  {}", label), Style::default().fg(DIM)),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    if !uuids.is_empty() {
        state.select(Some(app.browser.selected.min(uuids.len() - 1)));
    }
    f.render_stateful_widget(
        List::new(items)
            .block(hex_block(&format!("UUIDS ({})", uuids.len()), CYAN))
            .highlight_style(Style::default().bg(GOLD).fg(INK).add_modifier(Modifier::BOLD))
            .highlight_symbol("⬡ "),
        body[0],
        &mut state,
    );

    let detail_block = hex_block("DECODE → RESOLVE", GOLD);
    let detail_inner = detail_block.inner(body[1]);
    f.render_widget(detail_block, body[1]);
    let detail_lines: Vec<Line> = match &app.browser.detail {
        Some(res) => browser::resolution_lines(res),
        None => vec![
            Line::from(Span::styled(
                "select a UUID and press Enter —",
                Style::default().fg(DIM),
            )),
            Line::from(Span::styled(
                "decode with the gyst codec, then resolve",
                Style::default().fg(DIM),
            )),
            Line::from(Span::styled(
                "the record at that address. Honest misses",
                Style::default().fg(DIM),
            )),
            Line::from(Span::styled(
                "are shown as misses, never filled in.",
                Style::default().fg(DIM),
            )),
        ],
    };
    f.render_widget(
        Paragraph::new(detail_lines).wrap(Wrap { trim: true }),
        detail_inner,
    );

    // --- status + hints / editor ---
    let foot: Vec<Line> = if app.browser.edit.is_some() {
        let label = match app.browser.edit {
            Some(BrowserEdit::Paste) => "paste UUID",
            Some(BrowserEdit::DossierPath) => "dossier path",
            None => "",
        };
        vec![
            Line::from(vec![
                Span::styled(format!("{}: ", label), Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
                Span::styled(app.browser.edit_buffer.clone(), Style::default().fg(Color::White)),
                Span::styled("█", Style::default().fg(GOLD)),
            ]),
            Line::from(Span::styled("Enter confirms · Esc cancels", Style::default().fg(DIM))),
        ]
    } else {
        vec![
            Line::from(Span::styled(app.browser.status.clone(), Style::default().fg(GREEN))),
            Line::from(vec![
                Span::styled("Tab", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
                Span::styled(" source · ", Style::default().fg(DIM)),
                Span::styled("Enter", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
                Span::styled(" resolve · ", Style::default().fg(DIM)),
                Span::styled("p", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
                Span::styled(" paste · ", Style::default().fg(DIM)),
                Span::styled("o", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
                Span::styled(" dossier path · ", Style::default().fg(DIM)),
                Span::styled("Esc", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
                Span::styled(" close", Style::default().fg(DIM)),
            ]),
        ]
    };
    f.render_widget(Paragraph::new(foot), chunks[2]);
}

/// Dossier preview overlay: the compiled draft, in-panel. Nothing has left
/// the machine at this point; the operator trims before PUBLISH.
fn render_dossier(f: &mut Frame, area: Rect, app: &mut App) {
    let dlg = centered_rect(92, 88, area);
    f.render_widget(Clear, dlg);
    let block = hex_block("DOSSIER — PREVIEW (nothing has left the machine)", CYAN);
    let inner = block.inner(dlg);
    f.render_widget(block, dlg);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(6),    // payload
            Constraint::Length(2), // hints
        ])
        .split(inner);

    let (slug, payload, scroll) = match &app.dossier {
        Some(d) => (d.slug.clone(), d.payload.clone(), d.scroll),
        None => ("—".to_string(), "no dossier compiled".to_string(), 0),
    };
    let entry_count = payload.matches("### ").count();
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("slug: ", Style::default().fg(DIM)),
                Span::styled(slug, Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
                Span::styled(format!("   {} record-backed entries", entry_count), Style::default().fg(DIM)),
            ]),
            Line::from(Span::styled(
                "Ghost header + mermaid verified at publish · gaps shown as gaps",
                Style::default().fg(DIM),
            )),
        ]),
        chunks[0],
    );

    let lines: Vec<Line> = payload
        .lines()
        .skip(scroll)
        .take(chunks[1].height.saturating_sub(2) as usize)
        .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::White))))
        .collect();
    f.render_widget(Paragraph::new(lines), chunks[1]);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("p", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(" publish (explicit choice)   ", Style::default().fg(DIM)),
            Span::styled("PgUp/PgDn", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(" scroll   ", Style::default().fg(DIM)),
            Span::styled("Esc", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(" close", Style::default().fg(DIM)),
        ])),
        chunks[2],
    );
}

/// PUBLISH dialog: explicit choice every time — Gist / repo docs / cancel.
/// No silent defaults; Cancel is the default selection.
fn render_publish(f: &mut Frame, area: Rect, app: &mut App) {
    let dlg = centered_rect(72, 62, area);
    f.render_widget(Clear, dlg);
    let block = hex_block("PUBLISH DOSSIER — OPERATOR CHOICE", GOLD);
    let inner = block.inner(dlg);
    f.render_widget(block, dlg);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),    // explanation
            Constraint::Length(3), // buttons
            Constraint::Length(3), // status
        ])
        .split(inner);

    let slug = app.dossier.as_ref().map(|d| d.slug.clone()).unwrap_or_default();
    let repo_root = env::var("IMBUUID_DOSSIER_REPO").unwrap_or_else(|_| ".".to_string());
    let repo_line = format!(" — writes docs/dossier/{}.md under {}", slug, repo_root);
    let body = vec![
        Line::from("Publishing is operator-tapped, every time. Choose the destination:"),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Gist", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(
                " — posts via `gh gist create` (private gist). If gh is missing",
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![Span::styled(
            "  or fails, the payload lands in ~/.imbuuid/dossier-outbox/ instead.",
            Style::default().fg(DIM),
        )]),
        Line::from(vec![
            Span::styled("  Repo", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(repo_line, Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  The payload is verified first: Ghost Catalog header + mermaid, or no publish.",
            Style::default().fg(DIM),
        )]),
        Line::from(vec![Span::styled(
            "  No silent network, ever.",
            Style::default().fg(DIM),
        )]),
    ];
    f.render_widget(Paragraph::new(body).wrap(Wrap { trim: true }), chunks[0]);

    let btns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(chunks[1]);
    let style_for = |active: bool| {
        if active {
            Style::default().bg(GOLD).fg(INK).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(DIM)
        }
    };
    app.publish.gist_rect = btns[0];
    app.publish.repo_rect = btns[1];
    app.publish.cancel_rect = btns[2];
    f.render_widget(
        Paragraph::new("[ Post Gist ]")
            .style(style_for(app.publish.choice == PublishChoice::Gist))
            .alignment(Alignment::Center),
        btns[0],
    );
    f.render_widget(
        Paragraph::new("[ Write repo ]")
            .style(style_for(app.publish.choice == PublishChoice::Repo))
            .alignment(Alignment::Center),
        btns[1],
    );
    f.render_widget(
        Paragraph::new("[ Cancel ] (Esc)")
            .style(style_for(app.publish.choice == PublishChoice::Cancel))
            .alignment(Alignment::Center),
        btns[2],
    );

    let status_style = if app.publish.status.contains("failed") || app.publish.status.contains("could not") {
        Style::default().fg(RED)
    } else {
        Style::default().fg(GREEN)
    };
    f.render_widget(
        Paragraph::new(app.publish.status.clone())
            .style(status_style)
            .wrap(Wrap { trim: true }),
        chunks[2],
    );
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

    #[test]
    fn frame_contains_browser_and_dossier_tiles() {
        let mut app = test_app();
        let out = render_to_string(&mut app, 120, 40);
        for needle in ["UUID BROWSER", "DOSSIER", "decode → resolve", "compile · preview"] {
            assert!(out.contains(needle), "frame missing {:?}", needle);
        }
    }

    #[test]
    fn browser_opens_lists_store_uuids_and_closes() {
        let mut app = test_app();
        app.on_key(KeyCode::Char('b'));
        assert_eq!(app.mode, Mode::Browser);
        let out = render_to_string(&mut app, 120, 40);
        assert!(out.contains("UUID BROWSER — DECODE → RESOLVE"));
        assert!(out.contains("store"));
        // Store source lists memories + prompt fingerprints.
        let n = app.browser.current_uuids(&app.store).len();
        assert_eq!(n, app.store.memories().len() + app.store.prompts.len());
        assert!(n > 0);
        app.on_key(KeyCode::Esc);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn browser_resolve_selected_shows_record() {
        let mut app = test_app();
        app.on_key(KeyCode::Char('b'));
        app.on_key(KeyCode::Enter); // resolve first store UUID
        assert!(app.browser.detail.is_some());
        let out = render_to_string(&mut app, 120, 40);
        assert!(out.contains("local memory record"));
        assert!(out.contains("MEMORY (0x400)"));
    }

    #[test]
    fn browser_paste_honest_miss_path() {
        use imbuuid_mem::gyst::{encode_gyst, TYPE_MEMORY, PROV_CLI};
        let mut app = test_app();
        app.on_key(KeyCode::Char('b'));
        // Open the paste editor, type an unknown-but-valid v8 UUID, Enter.
        app.on_key(KeyCode::Char('p'));
        let uuid = encode_gyst(TYPE_MEMORY, 0x777, 0x111111, 0, 0, 0, 0.5, PROV_CLI, 99);
        for c in uuid.chars() {
            app.on_key(KeyCode::Char(c));
        }
        app.on_key(KeyCode::Enter);
        let out = render_to_string(&mut app, 120, 40);
        assert!(out.contains("no local record"));
        // The paste tab now lists the pasted UUID (source is already 2).
        assert_eq!(app.browser.source, 2);
        assert_eq!(app.browser.current_uuids(&app.store), vec![uuid]);
    }

    #[test]
    fn clicking_browser_and_dossier_tiles_opens_views() {
        let mut app = test_app();
        let _ = render_to_string(&mut app, 120, 40); // records tile rects
        let r = app.browser_tile_rect;
        assert!(r.width > 0, "browser tile rect recorded");
        app.on_click(r.x + 2, r.y + 2);
        assert_eq!(app.mode, Mode::Browser);
        app.on_key(KeyCode::Esc);
        let r = app.dossier_tile_rect;
        assert!(r.width > 0, "dossier tile rect recorded");
        app.on_click(r.x + 2, r.y + 2);
        assert_eq!(app.mode, Mode::Dossier);
    }

    #[test]
    fn dossier_preview_opens_and_shows_compiled_entries() {
        let mut app = test_app();
        app.on_key(KeyCode::Char('d'));
        assert_eq!(app.mode, Mode::Dossier);
        let d = app.dossier.as_ref().expect("dossier compiled");
        assert!(d.payload.contains("file_id: SOM-DOC-10435-v1.0.0 (part: dossier)"));
        assert!(d.payload.contains("```mermaid"));
        let out = render_to_string(&mut app, 120, 40);
        assert!(out.contains("DOSSIER — PREVIEW"));
        assert!(out.contains("record-backed entries"));
        app.on_key(KeyCode::Esc);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn publish_dialog_defaults_to_cancel_and_never_posts() {
        let mut app = test_app();
        app.on_key(KeyCode::Char('d'));
        app.on_key(KeyCode::Char('p'));
        assert_eq!(app.mode, Mode::Publish);
        assert_eq!(app.publish.choice, PublishChoice::Cancel, "safe default");
        let out = render_to_string(&mut app, 120, 40);
        assert!(out.contains("OPERATOR CHOICE"));
        assert!(out.contains("No silent network"));
        // Enter on Cancel returns to preview; nothing was published.
        app.on_key(KeyCode::Enter);
        assert_eq!(app.mode, Mode::Dossier);
        assert!(app.publish.status.is_empty(), "cancel publishes nothing");
    }

    #[test]
    fn publish_gist_choice_reports_without_network_in_test() {
        // Even when the operator picks Gist, the outbox path runs under
        // IMBUUID_DOSSIER_NO_GIST=1 — the dialog never posts silently.
        std::env::set_var("IMBUUID_DOSSIER_NO_GIST", "1");
        let dir = std::env::temp_dir().join("imbuuid-panel-ui-publish-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("IMBUUID_DOSSIER_OUTBOX", dir.to_str().unwrap());
        let mut app = test_app();
        app.on_key(KeyCode::Char('d'));
        app.on_key(KeyCode::Char('p'));
        // Tab twice: Cancel -> Gist.
        app.on_key(KeyCode::Tab);
        assert_eq!(app.publish.choice, PublishChoice::Gist);
        app.on_key(KeyCode::Enter);
        assert!(app.publish.status.contains("outbox"), "status: {}", app.publish.status);
        assert!(!app.publish.status.contains("posted GitHub Gist"));
        std::env::remove_var("IMBUUID_DOSSIER_NO_GIST");
        std::env::remove_var("IMBUUID_DOSSIER_OUTBOX");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn all_views_render_without_panic_at_small_size() {
        // 80x24 must not panic in any view (layouts degrade, never crash).
        let mut app = test_app();
        let _ = render_to_string(&mut app, 80, 24);
        app.on_key(KeyCode::Char('b'));
        let _ = render_to_string(&mut app, 80, 24);
        app.on_key(KeyCode::Enter);
        let _ = render_to_string(&mut app, 80, 24);
        app.on_key(KeyCode::Esc);
        app.on_key(KeyCode::Char('d'));
        let _ = render_to_string(&mut app, 80, 24);
        app.on_key(KeyCode::Char('p'));
        let _ = render_to_string(&mut app, 80, 24);
    }
}
