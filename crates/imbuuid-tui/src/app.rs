// file_id: SOM-DOC-10434 (part)
// name: src/app.rs
// description: TUI application state — filter-as-you-type search with 30ms
//   debounce, sortable memory list, keyboard + mouse event handling,
//   panel-chrome actions (Snap/Font/Capture/Help/Quit).
// project_id: IMBUUID-MEM
// category: tui
// created: 2026-10-17
// version: 1.0.0

use crate::backend::{self, MemoryView};
use crate::capture;
use crate::config::{self, Density, TuiConfig};
use crate::snapshot;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use imbuuid_mem::memory::MemoryStore;
use imbuuid_mem::witness::Witness;
use ratatui::layout::Rect;
use ratatui::widgets::TableState;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Filter-as-you-type debounce: keystrokes settle for this long before the
/// list re-filters. Matches the CCPC bar — every keystroke re-filters,
/// no enter-to-search, but coalesced so a burst of typing filters once.
pub const DEBOUNCE: Duration = Duration::from_millis(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    ObsDesc,
    ObsAsc,
    LastSeenDesc,
}

impl SortMode {
    pub fn next(self) -> SortMode {
        match self {
            SortMode::ObsDesc => SortMode::ObsAsc,
            SortMode::ObsAsc => SortMode::LastSeenDesc,
            SortMode::LastSeenDesc => SortMode::ObsDesc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortMode::ObsDesc => "obs↓",
            SortMode::ObsAsc => "obs↑",
            SortMode::LastSeenDesc => "seen↓",
        }
    }
}

/// Last-rendered clickable rects (title-bar chrome + table/detail areas).
#[derive(Debug, Clone, Default)]
pub struct ChromeRects {
    pub snap: Rect,
    pub font: Rect,
    pub capture: Rect,
    pub help: Rect,
    pub quit: Rect,
    pub table: Rect,
    pub detail: Rect,
}

pub struct App {
    pub store: MemoryStore,
    pub witness: Witness,
    pub views: Vec<MemoryView>,
    pub filtered: Vec<usize>,
    pub table_state: TableState,
    pub query: String,
    pub pending: Option<(String, Instant)>,
    pub search_focused: bool,
    pub sort: SortMode,
    pub density: Density,
    pub detail_mode: bool,
    pub detail_scroll: usize,
    pub show_help: bool,
    pub status: String,
    pub base_dir: PathBuf,
    pub store_path: PathBuf,
    pub ns_labels: HashMap<u16, String>,
    pub chrome: ChromeRects,
    pub should_quit: bool,
    pub pending_capture: bool,
    pub last_search_ms: u128,
    pub provenance_short: String,
}

impl App {
    pub fn new(
            store: MemoryStore,
            mut witness: Witness,
            base_dir: PathBuf,
            store_path: PathBuf,
            density: Density,
            ns_labels: HashMap<u16, String>,
        ) -> App {
        // Session witness: the v0 witness plane is in-memory, so re-observe
        // the loaded records once per session (ReObservation attestations).
        backend::reobserve(&store, &mut witness);
        let views = backend::load_views(&store, &ns_labels);
        let mut app = App {
            views,
            filtered: Vec::new(),
            table_state: TableState::default(),
            query: String::new(),
            pending: None,
            search_focused: false,
            sort: SortMode::ObsDesc,
            density,
            detail_mode: false,
            detail_scroll: 0,
            show_help: false,
            status: String::from("ready — / to search, ? for help"),
            base_dir,
            store_path,
            store,
            witness,
            ns_labels,
            chrome: ChromeRects::default(),
            should_quit: false,
            pending_capture: false,
            last_search_ms: 0,
            provenance_short: backend::provenance_short(),
        };
        app.apply_filter();
        if !app.filtered.is_empty() {
            app.table_state.select(Some(0));
        }
        let n = app.views.len();
        let m = backend::attestation_count(&app.witness);
        tracing::info!(
            memories = n,
            attestations = m,
            provenance = %app.provenance_short,
            store = %app.store_path.display(),
            "app start"
        );
        app
    }

    // ---- search ----

    /// Pure predicate: content substring + uuid-prefix + namespace.
    pub fn matches_query(v: &MemoryView, q: &str) -> bool {
        if q.is_empty() {
            return true;
        }
        let ql = q.to_lowercase();
        v.content.to_lowercase().contains(&ql)
            || v.namespace_label.to_lowercase().contains(&ql)
            || v.uuid
                .chars()
                .filter(|c| *c != '-')
                .take(ql.len().max(8))
                .collect::<String>()
                .to_lowercase()
                .starts_with(&ql)
            || v.uuid_short().to_lowercase().starts_with(&ql)
    }

    /// Pure debounce check, unit-testable.
    pub fn debounce_due(pending_at: Instant, now: Instant) -> bool {
        now.duration_since(pending_at) >= DEBOUNCE
    }

    pub fn apply_filter(&mut self) {
        let t0 = Instant::now();
        let q = self.query.clone();
        self.filtered = self
            .views
            .iter()
            .enumerate()
            .filter(|(_, v)| Self::matches_query(v, &q))
            .map(|(i, _)| i)
            .collect();
        self.sort_filtered();
        self.last_search_ms = t0.elapsed().as_millis();
        let count = self.filtered.len();
        // Keep selection valid.
        let sel = self.table_state.selected().unwrap_or(0).min(count.saturating_sub(1));
        self.table_state.select(if count == 0 { None } else { Some(sel) });
        tracing::info!(
            query = %q,
            results = count,
            ms = self.last_search_ms,
            "search applied"
        );
    }

    fn sort_filtered(&mut self) {
        let views = &self.views;
        match self.sort {
            SortMode::ObsDesc => self
                .filtered
                .sort_by(|&a, &b| views[b].observation_count.cmp(&views[a].observation_count)),
            SortMode::ObsAsc => self
                .filtered
                .sort_by(|&a, &b| views[a].observation_count.cmp(&views[b].observation_count)),
            SortMode::LastSeenDesc => self
                .filtered
                .sort_by(|&a, &b| views[b].last_seen_sec.cmp(&views[a].last_seen_sec)),
        }
    }

    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.sort_filtered();
        self.status = format!("sort: {}", self.sort.label());
        tracing::info!(sort = %self.sort.label(), "sort cycled");
    }

    pub fn selected_view(&self) -> Option<&MemoryView> {
        self.table_state
            .selected()
            .and_then(|i| self.filtered.get(i))
            .and_then(|&vi| self.views.get(vi))
    }

    /// Called each loop iteration: apply debounced filter when due.
    pub fn tick(&mut self) {
        if let Some((q, at)) = self.pending.take() {
            if Self::debounce_due(at, Instant::now()) {
                self.query = q;
                self.apply_filter();
            } else {
                self.pending = Some((q, at));
            }
        }
    }

    fn queue_filter(&mut self) {
        self.pending = Some((self.query.clone(), Instant::now()));
    }

    // ---- chrome actions (all honest: they do the thing, then confirm) ----

    pub fn do_snap(&mut self) {
        match snapshot::write_snapshot(self) {
            Ok(p) => {
                self.status = format!("snap → {}", p.display());
                tracing::info!(path = %p.display(), "snap action");
            }
            Err(e) => {
                self.status = format!("snap FAILED: {}", e);
                tracing::error!(error = %e, "snap action failed");
            }
        }
    }

    pub fn do_font(&mut self) {
        self.density = self.density.toggle();
        let cfg = TuiConfig { density: self.density };
        match config::save(&self.base_dir, &cfg) {
            Ok(()) => self.status = format!("density: {} (saved)", self.density.label()),
            Err(e) => self.status = format!("density: {} (save failed: {})", self.density.label(), e),
        }
        tracing::info!(density = %self.density.label(), "font action");
    }

    pub fn do_capture(&mut self) {
        self.pending_capture = true;
    }

    pub fn finish_capture(&mut self, frame_text: &str) {
        self.pending_capture = false;
        match capture::write_capture(&self.base_dir, frame_text) {
            Ok(p) => {
                self.status = format!("captured → {}", p.display());
                tracing::info!(path = %p.display(), "capture action");
            }
            Err(e) => {
                self.status = format!("capture FAILED: {}", e);
                tracing::error!(error = %e, "capture action failed");
            }
        }
    }

    pub fn do_refresh(&mut self) {
        // open_store takes the DIRECTORY (v0 snapshot lives inside it).
        match backend::open_store(&self.base_dir) {
            Ok(store) => {
                self.views = backend::load_views(&store, &self.ns_labels);
                self.store = store;
                // Fresh session witness, then re-observe the reloaded records.
                self.witness = backend::new_witness();
                backend::reobserve(&self.store, &mut self.witness);
                self.apply_filter();
                self.status = format!("refreshed — {} memories", self.views.len());
                tracing::info!(memories = self.views.len(), "refresh");
            }
            Err(e) => {
                self.status = format!("refresh FAILED: {}", e);
                tracing::error!(error = %e, "refresh failed");
            }
        }
    }

    // ---- input ----

    fn move_selection(&mut self, delta: i32) {
        if self.detail_mode || self.filtered.is_empty() {
            return;
        }
        let n = self.filtered.len();
        let cur = self.table_state.selected().unwrap_or(0) as i32;
        let next = (cur + delta).clamp(0, n as i32 - 1) as usize;
        self.table_state.select(Some(next));
        self.detail_scroll = 0;
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return;
        }
        // Help overlay eats everything except close keys.
        if self.show_help {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.show_help = false;
                }
                _ => {}
            }
            return;
        }
        // Search field editing.
        if self.search_focused {
            match key.code {
                KeyCode::Esc => {
                    self.search_focused = false;
                    self.status = "search unfocused".to_string();
                }
                KeyCode::Backspace => {
                    self.query.pop();
                    self.queue_filter();
                }
                KeyCode::Char(c) => {
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT)
                    {
                        self.query.push(c);
                        self.queue_filter();
                    }
                }
                KeyCode::Enter => {
                    self.search_focused = false;
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('/') => {
                self.search_focused = true;
                self.status = "search: type to filter (Esc to unfocus)".to_string();
            }
            KeyCode::Char('r') => self.do_refresh(),
            KeyCode::Char('s') => self.cycle_sort(),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Enter => {
                self.detail_mode = !self.detail_mode;
                self.detail_scroll = 0;
                self.status = if self.detail_mode {
                    "detail scroll mode — ↑/↓ scroll, Esc exits".to_string()
                } else {
                    "detail scroll off".to_string()
                };
            }
            KeyCode::Esc => {
                if self.detail_mode {
                    self.detail_mode = false;
                    self.status = "detail scroll off".to_string();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.detail_mode {
                    self.detail_scroll = self.detail_scroll.saturating_sub(1);
                } else {
                    self.move_selection(-1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.detail_mode {
                    self.detail_scroll = self.detail_scroll.saturating_add(1);
                } else {
                    self.move_selection(1);
                }
            }
            KeyCode::PageUp => {
                if self.detail_mode {
                    self.detail_scroll = self.detail_scroll.saturating_sub(10);
                } else {
                    self.move_selection(-10);
                }
            }
            KeyCode::PageDown => {
                if self.detail_mode {
                    self.detail_scroll = self.detail_scroll.saturating_add(10);
                } else {
                    self.move_selection(10);
                }
            }
            KeyCode::Home => {
                if !self.detail_mode && !self.filtered.is_empty() {
                    self.table_state.select(Some(0));
                    self.detail_scroll = 0;
                }
            }
            KeyCode::End => {
                if !self.detail_mode && !self.filtered.is_empty() {
                    self.table_state.select(Some(self.filtered.len() - 1));
                    self.detail_scroll = 0;
                }
            }
            KeyCode::F(5) => self.do_snap(),
            KeyCode::F(6) => self.do_font(),
            KeyCode::F(7) => self.do_capture(),
            _ => {}
        }
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        let (col, row) = (m.column, m.row);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let c = &self.chrome;
                let pt = Rect::new(col, row, 1, 1);
                if rect_hits(c.snap, pt) {
                    self.do_snap();
                } else if rect_hits(c.font, pt) {
                    self.do_font();
                } else if rect_hits(c.capture, pt) {
                    self.do_capture();
                } else if rect_hits(c.help, pt) {
                    self.show_help = true;
                } else if rect_hits(c.quit, pt) {
                    self.should_quit = true;
                } else if rect_hits(c.table, pt) {
                    // Select the clicked row: header occupies 3 lines (top border + header + separator).
                    let rh = self.density.row_height() as u16;
                    let rel = row.saturating_sub(c.table.y + 3);
                    let idx = (rel / rh) as usize + self.table_state.offset();
                    if idx < self.filtered.len() {
                        self.table_state.select(Some(idx));
                        self.detail_scroll = 0;
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if self.detail_mode {
                    self.detail_scroll = self.detail_scroll.saturating_sub(3);
                } else {
                    self.move_selection(-3);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.detail_mode {
                    self.detail_scroll = self.detail_scroll.saturating_add(3);
                } else {
                    self.move_selection(3);
                }
            }
            _ => {}
        }
    }
}

fn rect_hits(r: Rect, pt: Rect) -> bool {
    r.width > 0 && pt.x >= r.x && pt.x < r.x + r.width && pt.y >= r.y && pt.y < r.y + r.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MemoryView;

    fn v(uuid: &str, content: &str, namespace: &str, obs: u64, last: u64) -> MemoryView {
        MemoryView {
            uuid: uuid.to_string(),
            content: content.to_string(),
            namespace_label: namespace.to_string(),
            namespace_raw: 0,
            observation_count: obs,
            signal: 0.5,
            first_seen_sec: 0,
            last_seen_sec: last,
            content_hash: 0,
            prompt_id: None,
            type_name: "memory".into(),
            provenance_name: "cli".into(),
            fractal_dgd: "d0/dom0/g0".into(),
            fractal_sector_raw: 0,
        }
    }

    #[test]
    fn matches_content_substring() {
        let m = v("4005aae7-7800-8000-89ff-feaaaaaaaaab", "proton deploy checklist", "ops", 1, 1);
        assert!(App::matches_query(&m, "proton"));
        assert!(App::matches_query(&m, "PROTON"));
        assert!(!App::matches_query(&m, "kalshi"));
    }

    #[test]
    fn matches_uuid_prefix() {
        let m = v("4005aae7-7800-8000-89ff-feaaaaaaaaab", "something", "ops", 1, 1);
        assert!(App::matches_query(&m, "4005"));
        assert!(App::matches_query(&m, "4005aae7"));
        assert!(!App::matches_query(&m, "9999"));
    }

    #[test]
    fn matches_namespace() {
        let m = v("4005aae7-7800-8000-89ff-feaaaaaaaaab", "something", "research", 1, 1);
        assert!(App::matches_query(&m, "research"));
        assert!(App::matches_query(&m, "resear"));
        assert!(!App::matches_query(&m, "ops"));
    }

    #[test]
    fn empty_query_matches_everything() {
        let m = v("x", "y", "z", 1, 1);
        assert!(App::matches_query(&m, ""));
    }

    #[test]
    fn debounce_due_at_30ms_boundary() {
        let t0 = Instant::now();
        assert!(!App::debounce_due(t0, t0 + Duration::from_millis(10)));
        assert!(App::debounce_due(t0, t0 + Duration::from_millis(30)));
        assert!(App::debounce_due(t0, t0 + Duration::from_millis(500)));
    }

    #[test]
    fn sort_mode_cycles_in_order() {
        assert_eq!(SortMode::ObsDesc.next(), SortMode::ObsAsc);
        assert_eq!(SortMode::ObsAsc.next(), SortMode::LastSeenDesc);
        assert_eq!(SortMode::LastSeenDesc.next(), SortMode::ObsDesc);
    }
}
