// file_id: SOM-DOC-10434 (part)
// name: src/ui.rs
// description: ratatui rendering — title bar with provenance + clickable
//   panel chrome, search bar, sortable memory table, detail pane, status +
//   key bars, help overlay; render_to_text() replays the same draw into a
//   TestBackend for F7 text captures and headless tests.
// project_id: IMBUUID-MEM
// category: tui
// created: 2026-10-17
// version: 1.0.0

use crate::app::{App, ChromeRects};
use crate::backend;
use chrono::{TimeZone, Utc};
use ratatui::backend::TestBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap};
use ratatui::{Frame, Terminal};

fn fmt_ts(ts: i64) -> String {
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn chrome_labels() -> [&'static str; 5] {
    ["[Snap]", "[Font]", "[Capture]", "[Help]", "[Quit]"]
}

/// Draw the full UI. Also records clickable chrome rects into app.chrome.
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Length(3), // search
            Constraint::Min(0),    // main
            Constraint::Length(1), // status
            Constraint::Length(1), // keybar
        ])
        .split(area);

    draw_title(f, app, chunks[0]);
    draw_search(f, app, chunks[1]);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[2]);
    app.chrome.table = main[0];
    app.chrome.detail = main[1];
    draw_table(f, app, main[0]);
    draw_detail(f, app, main[1]);

    draw_status(f, app, chunks[3]);
    draw_keybar(f, app, chunks[4]);

    if app.show_help {
        draw_help(f, area);
    }
}

fn draw_title(f: &mut Frame, app: &mut App, area: Rect) {
    let w = area.width as usize;
    let n = app.views.len();
    let m = backend::attestation_count(&app.witness);
    let left = format!(
        "imbuuid-tui — {} memories | {} attestations | store: {}",
        n,
        m,
        app.store_path.display()
    );
    let right = format!(
        "{}  prov:{}",
        chrome_labels().join(" "),
        app.provenance_short
    );

    // Right-aligned chrome + provenance; left truncated to fit.
    let right_len = right.chars().count().min(w);
    let left_max = w.saturating_sub(right_len + 1);
    let left_cut: String = left.chars().take(left_max).collect();
    let pad = w.saturating_sub(left_cut.chars().count() + right_len);

    // Record clickable chrome rects (buttons are the first 37 chars of `right`).
    // right = chrome(37) + "  "(2) + "prov:"(5) + short
    let prov_len = app.provenance_short.chars().count() as u16;
    let chrome_x = area.width.saturating_sub(37 + 2 + 5 + prov_len);
    let mut x = chrome_x;
    let mut rects = ChromeRects::default();
    let widths = [6u16, 6, 9, 6, 6];
    let slots = [
        &mut rects.snap,
        &mut rects.font,
        &mut rects.capture,
        &mut rects.help,
        &mut rects.quit,
    ];
    for (slot, bw) in slots.into_iter().zip(widths) {
        *slot = Rect::new(area.x + x, area.y, bw, 1);
        x += bw + 1;
    }
    rects.table = app.chrome.table;
    rects.detail = app.chrome.detail;
    app.chrome = rects;

    let mut spans = vec![Span::styled(left_cut, Style::default().add_modifier(Modifier::BOLD))];
    spans.push(Span::raw(" ".repeat(pad)));
    // Re-emit right side with per-button highlight so clicks read as buttons.
    for (i, label) in chrome_labels().into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            label,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("prov:{}", app.provenance_short),
        dim(),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let title = if app.search_focused {
        " Search — type to filter (Esc unfocus) "
    } else {
        " Search (/ to focus) "
    };
    let mut spans = vec![Span::raw(app.query.clone())];
    if app.search_focused {
        spans.push(Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)));
    }
    let style = if app.search_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let p = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL).title(title).border_style(style));
    f.render_widget(p, area);
}

fn draw_table(f: &mut Frame, app: &mut App, area: Rect) {
    let rh = app.density.row_height();
    let header = Row::new(vec![
        Cell::from("UUID").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Content").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Obs#").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Sig").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Last seen").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Namespace").style(Style::default().add_modifier(Modifier::BOLD)),
    ])
    .height(1);

    let rows: Vec<Row> = app
        .filtered
        .iter()
        .map(|&vi| {
            let v = &app.views[vi];
            let cells = if rh == 1 {
                vec![
                    Cell::from(v.uuid_short()).style(Style::default().fg(Color::Green)),
                    Cell::from(v.preview(40)),
                    Cell::from(format!("{}", v.observation_count)),
                    Cell::from(format!("{:.2}", v.signal)),
                    Cell::from(fmt_ts(v.last_seen_sec as i64)),
                    Cell::from(v.namespace_label.clone()).style(dim()),
                ]
            } else {
                vec![
                    Cell::from(Text::from(vec![
                        Line::from(Span::styled(
                            v.uuid_short(),
                            Style::default().fg(Color::Green),
                        )),
                        Line::from(Span::styled(
                            format!("{} · {}", v.type_name, v.provenance_name),
                            dim(),
                        )),
                    ])),
                    Cell::from(Text::from(vec![
                        Line::from(v.preview(40)),
                        Line::from(Span::styled(
                            format!(
                                "prompt:{}",
                                v.prompt_id
                                    .as_deref()
                                    .map(|p| p
                                        .chars()
                                        .filter(|c| *c != '-')
                                        .take(8)
                                        .collect::<String>())
                                    .unwrap_or_else(|| "—".to_string())
                            ),
                            dim(),
                        )),
                    ])),
                    Cell::from(format!("{}", v.observation_count)),
                    Cell::from(format!("{:.2}", v.signal)),
                    Cell::from(fmt_ts(v.last_seen_sec as i64)),
                    Cell::from(v.namespace_label.clone()).style(dim()),
                ]
            };
            Row::new(cells).height(rh)
        })
        .collect();

    let title = format!(
        " Memories ({}/{}) — sort:{} ",
        app.filtered.len(),
        app.views.len(),
        app.sort.label()
    );
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(17),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title))
    .row_highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
    .highlight_symbol("▶ ");
    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn detail_text(app: &App) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    let Some(v) = app.selected_view() else {
        lines.push(Line::from(Span::styled("no memory selected", dim())));
        return lines;
    };
    let kv = |k: &str, val: String| {
        Line::from(vec![
            Span::styled(format!("{:>12} ", k), dim()),
            Span::raw(val),
        ])
    };
    lines.push(kv("UUID", v.uuid.clone()));
    lines.push(kv("type", format!("{} ({:#x})", v.type_name, type_code_of(v))));
    lines.push(kv("hash", format!("{:016x}", v.content_hash)));
    lines.push(kv(
        "namespace",
        format!("{} ({:#06x})", v.namespace_label, v.namespace_raw),
    ));
    lines.push(kv("signal", format!("{:.4}", v.signal)));
    lines.push(kv("obs", format!("{}", v.observation_count)));
    lines.push(kv("first", fmt_ts(v.first_seen_sec as i64)));
    lines.push(kv("last", fmt_ts(v.last_seen_sec as i64)));
    lines.push(kv(
        "prompt",
        v.prompt_id
            .as_ref()
            .map(|p| p.chars().filter(|c| *c != '-').take(8).collect::<String>())
            .unwrap_or_else(|| "—".to_string()),
    ));
    lines.push(kv("prov", v.provenance_name.clone()));
    lines.push(kv(
        "fractal",
        format!("{} · sector {:#06x}", v.fractal_dgd, v.fractal_sector_raw),
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "content",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    for chunk in textwrap_lines(&v.content, 64) {
        lines.push(Line::from(chunk));
    }
    lines.push(Line::from(""));

    // Attestation receipts (last 3 by ts).
    let mut atts = backend::attestations_for(&app.witness, &v.uuid);
    atts.sort_by_key(|a| a.ts);
    lines.push(Line::from(Span::styled(
        format!("attestations ({})", atts.len()),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    if atts.is_empty() {
        lines.push(Line::from(Span::styled("  — none —", dim())));
    } else {
        for a in atts.iter().rev().take(3) {
            let rcpt: String = a.receipt.chars().take(8).collect();
            lines.push(Line::from(format!(
                "  [{}] {} obs={} {}",
                a.kind,
                fmt_ts(a.ts as i64),
                a.observation_count,
                rcpt
            )));
        }
    }
    lines.push(Line::from(""));

    // Related via the retrieval plane.
    let rel = backend::related(&app.store, v, &app.ns_labels, 3);
    lines.push(Line::from(Span::styled(
        "related (retrieval)",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    if rel.is_empty() {
        lines.push(Line::from(Span::styled("  — none —", dim())));
    } else {
        for (rv, score) in rel {
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", rv.uuid_short()), Style::default().fg(Color::Green)),
                Span::raw(format!("{:.2} ", score)),
                Span::styled(rv.preview(34), dim()),
            ]));
        }
    }
    lines
}

/// Re-decode the type code for display; best-effort.
fn type_code_of(v: &crate::backend::MemoryView) -> u16 {
    imbuuid_mem::gyst::decode_gyst(&v.uuid)
        .map(|d| d.type_code)
        .unwrap_or(0)
}

fn textwrap_lines(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            out.push(cur);
            cur = String::new();
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let border_style = if app.detail_mode {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let title = if app.detail_mode {
        " Detail — scroll mode (Esc exits) "
    } else {
        " Detail (Enter to scroll) "
    };
    let p = Paragraph::new(detail_text(app))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll as u16, 0));
    f.render_widget(p, area);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let text = if app.last_search_ms > 0 && !app.query.is_empty() {
        format!(
            "{}  ·  filter '{}' → {} in {}ms",
            app.status,
            app.query,
            app.filtered.len(),
            app.last_search_ms
        )
    } else {
        app.status.clone()
    };
    f.render_widget(Paragraph::new(Line::from(Span::styled(text, dim()))), area);
}

fn draw_keybar(f: &mut Frame, _app: &App, area: Rect) {
    let keys: Vec<(&str, &str)> = vec![
        ("q", "Quit"),
        ("/", "Search"),
        ("r", "Refresh"),
        ("s", "Sort"),
        ("?", "Help"),
        ("Enter", "Detail"),
        ("F5", "Snap"),
        ("F6", "Font"),
        ("F7", "Capture"),
    ];
    let mut spans = Vec::new();
    for (i, (k, label)) in keys.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(
            *k,
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(" {}", label)));
    }
    spans.push(Span::styled("   ↑↓/jk navigate", dim()));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn draw_help(f: &mut Frame, area: Rect) {
    let area = centered_rect(70, 80, area);
    f.render_widget(Clear, area);
    let help: Vec<Line> = vec![
        Line::from(Span::styled("imbuuid-tui — keys", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  /            focus search (filter-as-you-type, 30ms debounce)"),
        Line::from("  Esc          unfocus search / exit detail scroll / close help"),
        Line::from("  ↑↓ / j/k    move selection (PgUp/PgDn jump 10, Home/End)"),
        Line::from("  Enter        toggle detail-pane scroll mode"),
        Line::from("  s            cycle sort: obs↓ → obs↑ → last-seen↓"),
        Line::from("  r            refresh — reload store from disk"),
        Line::from("  F5           [Snap] snapshot view+store → ~/.imbuuid/snapshots/"),
        Line::from("  F6           [Font] cycle density compact ↔ comfortable (persisted)"),
        Line::from("  F7           [Capture] text screenshot → ~/.imbuuid/captures/"),
        Line::from("  ?            this help"),
        Line::from("  q            quit"),
        Line::from(""),
        Line::from("  mouse        click [Snap][Font][Capture][Help][Quit], click a row"),
        Line::from("               to select, wheel to scroll"),
        Line::from(""),
        Line::from(Span::styled("  ? / Esc / q closes", dim())),
    ];
    let p = Paragraph::new(help)
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .alignment(Alignment::Left);
    f.render_widget(p, area);
}

/// Render one frame to plain text (for F7 captures and headless tests).
pub fn render_to_text(app: &mut App, area: Rect) -> String {
    let backend = TestBackend::new(area.width.max(1), area.height.max(1));
    let mut terminal = Terminal::new(backend).expect("test backend");
    terminal
        .draw(|f| draw(f, app))
        .expect("draw to test backend");
    let buf = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buf.area.height {
        let mut line = String::new();
        for x in 0..buf.area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MemoryView;

    fn test_app() -> App {
        let dir = std::env::temp_dir().join(format!(
            "imbuuid-tui-uitest-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store_path = dir.join("memories.json");
        let store = crate::backend::open_store(&store_path).unwrap();
        let witness = crate::backend::new_witness();
        let mut ns_labels = std::collections::HashMap::new();
        crate::backend::register_label(&mut ns_labels, "ops");
        let mut app = App::new(
            store,
            witness,
            dir,
            store_path,
            crate::config::Density::Compact,
            ns_labels,
        );
        app.views = vec![MemoryView {
            uuid: "4005aae7-7800-8000-89ff-feaaaaaaaaab".to_string(),
            content: "proton deploy checklist".to_string(),
            namespace_label: "ops".to_string(),
            namespace_raw: 0,
            observation_count: 7,
            signal: 0.92,
            first_seen_sec: 1760000000,
            last_seen_sec: 1760500000,
            content_hash: 0xabcdef0123456789,
            prompt_id: Some("abc".to_string()),
            type_name: "memory".to_string(),
            provenance_name: "cli".to_string(),
            fractal_dgd: "d0/dom0/g0".to_string(),
            fractal_sector_raw: 0,
        }];
        app.apply_filter();
        app.table_state.select(Some(0));
        app
    }

    #[test]
    fn headless_frame_contains_title_table_and_detail() {
        let mut app = test_app();
        let text = render_to_text(&mut app, Rect::new(0, 0, 120, 40));
        assert!(text.contains("imbuuid-tui"), "title missing");
        assert!(text.contains("4005aae7"), "uuid short missing");
        assert!(text.contains("proton deploy checklist"), "content missing");
        assert!(text.contains("Detail"), "detail pane missing");
        assert!(text.contains("[Snap]"), "chrome missing");
        assert!(text.contains("Quit"), "keybar missing");
        assert!(text.contains("prov:"), "provenance missing from title");
        assert!(text.contains("25 memories") || text.contains("1 memories"),
            "memory count missing from title");
    }

    #[test]
    fn fmt_ts_formats_unix_epoch_sanely() {
        assert_eq!(fmt_ts(1760000000), "2025-10-09 08:53");
    }
}
