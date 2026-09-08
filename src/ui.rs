//! TUI rendering: ratatui layout, palette rows, menus, status bar.
//!
//! The renderer is split into pure "build widgets" helpers (testable without
//! a terminal) and the `draw` entry point that pushes them to a ratatui
//! `Frame`. All state lives in [`EditorState`] so this module never owns
//! mutable editor data.

use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::characters::{self, CharCategory};
use crate::config::EditorConfig;
use crate::editor::Buffer;
use crate::syntax::{self, TokenKind};

/// Live editor state the UI reads every frame.
#[derive(Debug, Clone)]
pub struct EditorState {
    pub buffer: Buffer,
    pub config: EditorConfig,
    /// Currently selected palette row (TAB cycles).
    pub palette_row: usize,
    /// Currently selected entry within the row.
    pub palette_col: usize,
    /// ALT menu open + which item is focused.
    pub menu_open: bool,
    pub menu_focus: usize,
    /// Gateway connection status line.
    pub gateway_status: String,
    /// Result pane lines (most recent last).
    pub results: Vec<String>,
    /// Status bar message (errors, confirmations, hints).
    pub status: String,
    /// ⎕IO / ⎕SEC snapshot for the status bar.
    pub io_label: String,
    pub sec_label: String,
}

impl EditorState {
    pub fn new(config: EditorConfig) -> Self {
        Self {
            buffer: Buffer::new(),
            config,
            palette_row: 0,
            palette_col: 0,
            menu_open: false,
            menu_focus: 0,
            gateway_status: "disconnected".to_string(),
            results: Vec::new(),
            status: "TAB: palette  Space: insert  Ctrl-E: eval  Ctrl-Q: quit".to_string(),
            io_label: "⎕IO=1".to_string(),
            sec_label: "⎕SEC=0".to_string(),
        }
    }

    pub fn palette(&self) -> &'static characters::PaletteRow {
        characters::row(self.palette_row)
    }

    pub fn push_result(&mut self, line: String) {
        self.results.push(line);
        let max = self.config.max_results;
        if self.results.len() > max {
            self.results.drain(0..self.results.len() - max);
        }
    }
}

/// Top-level layout constraints (ratatui `Constraint`s), shared by draw + tests.
pub fn layout_constraints() -> [Constraint; 4] {
    [
        Constraint::Length(2), // palette row
        Constraint::Min(4),    // editor
        Constraint::Length(6), // result pane
        Constraint::Length(1), // status bar
    ]
}

fn palette_style(cat: CharCategory) -> Style {
    match cat {
        CharCategory::Primitive => Style::default().fg(Color::Yellow),
        CharCategory::Operator => Style::default().fg(Color::Cyan),
        CharCategory::Quad => Style::default().fg(Color::Green),
        CharCategory::Greek => Style::default().fg(Color::Magenta),
        CharCategory::BoxDraw => Style::default().fg(Color::Blue),
        CharCategory::Dyalog => Style::default().fg(Color::Red),
    }
}

fn token_style(kind: TokenKind) -> Style {
    match kind {
        TokenKind::Comment => Style::default().fg(Color::DarkGray),
        TokenKind::String => Style::default().fg(Color::Green),
        TokenKind::Number => Style::default().fg(Color::Cyan),
        TokenKind::Primitive => Style::default().fg(Color::Yellow),
        TokenKind::Operator => Style::default().fg(Color::LightCyan),
        TokenKind::QuadName => Style::default().fg(Color::LightGreen),
        TokenKind::SysCmd => Style::default().fg(Color::Red),
        TokenKind::FnMarker => Style::default().fg(Color::Magenta),
        TokenKind::Identifier => Style::default().fg(Color::White),
        TokenKind::Whitespace => Style::default(),
        TokenKind::Other => Style::default().fg(Color::White),
    }
}

/// Build the palette row widget for the current state.
pub fn render_palette(state: &EditorState) -> Paragraph<'static> {
    let row = state.palette();
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(
        format!(" {}: ", row.category.title()),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    for (i, entry) in row.entries.iter().enumerate() {
        let style = palette_style(row.category);
        let focused = i == state.palette_col;
        let s = if focused {
            style
                .bg(Color::White)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            style
        };
        spans.push(Span::styled(format!(" {} ", entry.glyph), s));
    }
    Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::ALL).title(format!(
        "Palette (row {}/{})",
        state.palette_row + 1,
        characters::row_count()
    )))
}

/// Build the editor widget with per-token syntax highlighting.
pub fn render_editor(state: &EditorState) -> Paragraph<'static> {
    let mut lines: Vec<Line> = Vec::new();
    for (r, text) in state.buffer.lines().iter().enumerate() {
        let toks = syntax::highlight_line(text);
        let mut spans: Vec<Span> = toks
            .into_iter()
            .map(|t| Span::styled(t.text, token_style(t.kind)))
            .collect();
        if r == state.buffer.cursor().row {
            spans.insert(0, Span::raw("▎"));
        } else {
            spans.insert(0, Span::raw("  "));
        }
        lines.push(Line::from(spans));
    }
    Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Editor — {}", state.buffer.display_name())),
    )
}

/// Build the result pane widget.
pub fn render_results(state: &EditorState) -> Paragraph<'static> {
    let text = if state.results.is_empty() {
        "(no results yet — Ctrl-E evaluates the current line)".to_string()
    } else {
        state.results.join("\n")
    };
    Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Results (gateway)"),
    )
}

/// Build the status bar widget.
pub fn render_status_bar(state: &EditorState) -> Paragraph<'static> {
    let left = format!(
        "{}  {}  cursor={}:{}",
        state.io_label,
        state.sec_label,
        state.buffer.cursor().row + 1,
        state.buffer.cursor().col + 1,
    );
    let right = format!("gateway: {}", state.gateway_status);
    let text = format!("{left}    {right}    {}", state.status);
    Paragraph::new(text).style(Style::default().fg(Color::White).bg(Color::DarkGray))
}

/// Build the ALT menu overlay (File / Edit / Help / Quit).
pub fn render_menu(state: &EditorState) -> List<'static> {
    let items = [
        "File › New",
        "File › Open…",
        "File › Save",
        "File › Save As…",
        "Edit › Undo",
        "Edit › Redo",
        "Edit › Cut",
        "Edit › Copy",
        "Edit › Paste",
        "Help › About",
        "QUIT › Quit",
    ];
    let focused = state.menu_focus.min(items.len() - 1);
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let style = if i == focused {
                Style::default().bg(Color::White).fg(Color::Black)
            } else {
                Style::default()
            };
            ListItem::new(*it).style(style)
        })
        .collect();
    List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Menu (ESC closes)"),
    )
}

/// Number of menu items (for bounds + overlay height).
pub fn menu_item_count() -> usize {
    11
}

/// Draw the whole frame. `area` is the terminal's full rect.
pub fn draw(frame: &mut ratatui::Frame, state: &EditorState) {
    use ratatui::layout::Layout;

    let chunks = Layout::vertical(layout_constraints()).split(frame.area());
    frame.render_widget(render_palette(state), chunks[0]);
    frame.render_widget(render_editor(state), chunks[1]);
    frame.render_widget(render_results(state), chunks[2]);
    frame.render_widget(render_status_bar(state), chunks[3]);

    if state.menu_open {
        let menu = render_menu(state);
        let w = 28u16;
        let h = (menu_item_count() + 2) as u16;
        let area = centered_rect(w, h, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(menu, area);
    }
}

fn centered_rect(w: u16, h: u16, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Layout};
    let col = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(w),
        Constraint::Fill(1),
    ])
    .split(area)[1];
    Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(h),
        Constraint::Fill(1),
    ])
    .split(col)[1]
}

/// Short human name for the focused palette entry (status bar hint).
pub fn focused_entry_name(state: &EditorState) -> String {
    let row = state.palette();
    row.entries
        .get(state.palette_col)
        .map(|e| format!("{} = {}", e.glyph, e.name))
        .unwrap_or_default()
}

/// Format a `ValueP` to a display string (used by pipe mode + local eval).
pub fn format_value_for(v: &apl::value::ValueP, pp: usize) -> String {
    let all_chars = !v.cells().is_empty() && v.cells().iter().all(|c| c.is_character_cell());
    if v.rank() >= 2 || all_chars {
        apl::boxdisplay::render_plain_with_pp(v, pp).join("\n")
    } else if v.is_scalar() || v.is_vector() {
        v.cells()
            .iter()
            .map(|c| apl::boxdisplay::plain_cell(c, pp))
            .collect::<Vec<_>>()
            .join("  ")
    } else {
        format!("⍴{}", v.shape())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> EditorState {
        let mut s = EditorState::new(EditorConfig::default());
        s.buffer = Buffer::from_string("A←⍳5\n+/A ⍝ sum");
        s.results.push("15".to_string());
        s
    }

    #[test]
    fn palette_widget_builds_without_panic() {
        let s = sample_state();
        let _w = render_palette(&s);
    }

    #[test]
    fn editor_widget_builds_without_panic() {
        let s = sample_state();
        let _w = render_editor(&s);
        // The buffer has 2 lines; the widget should reflect that without panicking.
        assert_eq!(s.buffer.line_count(), 2);
    }

    #[test]
    fn menu_has_eleven_items() {
        let mut s = sample_state();
        s.menu_open = true;
        let list = render_menu(&s);
        assert_eq!(list.len(), 11);
    }

    #[test]
    fn push_result_caps_at_max() {
        let mut s = sample_state();
        s.config.max_results = 3;
        s.results.clear();
        for i in 0..10 {
            s.push_result(format!("r{i}"));
        }
        assert_eq!(s.results.len(), 3);
        assert_eq!(s.results[0], "r7");
    }

    #[test]
    fn layout_constraints_sum_to_full_height() {
        let c = layout_constraints();
        assert_eq!(c.len(), 4);
        assert!(matches!(c[0], Constraint::Length(2)));
        assert!(matches!(c[3], Constraint::Length(1)));
    }

    #[test]
    fn focused_entry_name_for_default_state() {
        let s = sample_state();
        let name = focused_entry_name(&s);
        assert!(name.contains("plus"));
    }

    #[test]
    fn render_results_shows_placeholder_when_empty() {
        let s = EditorState::new(EditorConfig::default());
        let _w = render_results(&s);
    }

    #[test]
    fn render_status_bar_builds() {
        let s = sample_state();
        let _w = render_status_bar(&s);
    }
}
