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
pub struct EditorState {
    pub buffers: Vec<Buffer>,
    pub active_buffer: usize,
    pub config: EditorConfig,
    /// Currently selected palette row (TAB cycles).
    pub palette_row: usize,
    /// Currently selected entry within the row.
    pub palette_col: usize,
    /// Palette expanded to show 5 rows at once (Ctrl+P toggles).
    pub palette_expanded: bool,
    /// ESC menu open + which item is focused.
    pub menu_open: bool,
    pub menu_focus: usize,
    /// File open dialog state.
    pub dialog: Option<Dialog>,
    /// Gateway connection status line.
    pub gateway_status: String,
    /// Result pane lines (most recent last).
    pub results: Vec<String>,
    /// Status bar message (errors, confirmations, hints).
    pub status: String,
    /// ⎕IO / ⎕SEC snapshot for the status bar.
    pub io_label: String,
    pub sec_label: String,
    /// Local interpreter environment (persists across evaluations when no gateway).
    pub env: apl::parser::Environment,
    /// Results pane display mode: 1=compact, 2=expanded (50/50 horizontal), 3=split (50/50 vertical).
    pub results_mode: u8,
}

#[derive(Debug, Clone)]
pub enum Dialog {
    OpenFile {
        path: String,
        cursor: usize,        // selected file index
        files: Vec<String>,   // files in current directory
    },
    SaveAs {
        path: String,
    },
    Help {
        scroll: usize,
    },
}

impl EditorState {
    pub fn buffer(&self) -> &Buffer {
        &self.buffers[self.active_buffer]
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.active_buffer]
    }

    pub fn new(config: EditorConfig) -> Self {
        Self {
            buffers: vec![Buffer::new()],
            active_buffer: 0,
            config,
            palette_row: 0,
            palette_col: 0,
            palette_expanded: false,
            menu_open: false,
            menu_focus: 0,
            dialog: None,
            gateway_status: "disconnected".to_string(),
            results: Vec::new(),
            status: "TAB: next category  ←→: move cursor  Ctrl+←→: select glyph  Ctrl+Space: insert  Ctrl+P: palette  Ctrl+L: results mode  Ctrl+N: new buffer  Ctrl+O: open  Ctrl+Enter: run all  ESC: menu  Ctrl-E: eval  Ctrl-X: quit".to_string(),
            io_label: "⎕IO=1".to_string(),
            sec_label: "⎕SEC=0".to_string(),
            env: apl::parser::Environment::new(),
            results_mode: 1,
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
/// `palette_rows` is 1 normally, or 5 when expanded.
pub fn layout_constraints(palette_rows: usize) -> Vec<Constraint> {
    vec![
        Constraint::Length(palette_rows as u16), // palette
        Constraint::Min(4),                      // editor
        Constraint::Length(6),                   // result pane
        Constraint::Length(1),                   // status bar
    ]
}

fn palette_style(cat: CharCategory) -> Style {
    match cat {
        CharCategory::Assign => Style::default().fg(Color::Yellow),
        CharCategory::Arithmetic => Style::default().fg(Color::Cyan),
        CharCategory::Magnitude => Style::default().fg(Color::Green),
        CharCategory::Compare => Style::default().fg(Color::Magenta),
        CharCategory::Logical => Style::default().fg(Color::Blue),
        CharCategory::Structural => Style::default().fg(Color::Red),
        CharCategory::Membership => Style::default().fg(Color::LightCyan),
        CharCategory::Catenate => Style::default().fg(Color::LightGreen),
        CharCategory::Operators => Style::default().fg(Color::LightMagenta),
        CharCategory::Punctuation => Style::default().fg(Color::LightBlue),
        CharCategory::Misc => Style::default().fg(Color::LightRed),
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

/// Build the palette widget. Shows 3 rows normally, or 7 rows when expanded.
/// Rows are circular — the current row is always visible, and the view
/// wraps around seamlessly when cycling past the last/first row.
pub fn render_palette(state: &EditorState) -> Paragraph<'static> {
    let mut lines: Vec<Line> = Vec::new();
    let row_count = characters::row_count();

    if state.palette_expanded {
        // Show 7 rows with the current row always second (index 1).
        // Rows wrap around circularly.
        for offset in -1..=5 {
            let r = ((state.palette_row as isize + offset).rem_euclid(row_count as isize)) as usize;
            let row = characters::row(r);
            let is_current = r == state.palette_row;
            let mut spans: Vec<Span> = Vec::new();
            spans.push(Span::styled(
                format!(" {}: ", row.category.title()),
                if is_current {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ));
            for (i, entry) in row.entries.iter().enumerate() {
                let style = palette_style(row.category);
                let focused = is_current && i == state.palette_col;
                let s = if focused {
                    style
                        .bg(Color::White)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else if is_current {
                    style
                } else {
                    style.fg(Color::DarkGray)
                };
                spans.push(Span::styled(format!(" {} ", entry.glyph), s));
            }
            lines.push(Line::from(spans));
        }
    } else {
        // Show 3 rows: one above, current, one below. Current is always centered.
        for offset in -1..=1 {
            let r = ((state.palette_row as isize + offset).rem_euclid(row_count as isize)) as usize;
            let row = characters::row(r);
            let is_current = r == state.palette_row;
            let mut spans: Vec<Span> = Vec::new();
            spans.push(Span::styled(
                format!(" {}: ", row.category.title()),
                if is_current {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ));
            for (i, entry) in row.entries.iter().enumerate() {
                let style = palette_style(row.category);
                let focused = is_current && i == state.palette_col;
                let s = if focused {
                    style
                        .bg(Color::White)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else if is_current {
                    style
                } else {
                    style.fg(Color::DarkGray)
                };
                spans.push(Span::styled(format!(" {} ", entry.glyph), s));
            }
            lines.push(Line::from(spans));
        }
    }

    let title = if state.palette_expanded {
        "Palette (expanded — Ctrl+P to collapse)".to_string()
    } else {
        "Palette  ←→ next category: Tab |  move cursor: ctrl-leftArr, ctrl-rightArr | insert selection:  Ctrl+Space | expand palette:  Ctrl+P".to_string()
    };

    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title))
}

/// Build the editor widget with per-token syntax highlighting.
/// The cursor row shows an underscore at the cursor column.
pub fn render_editor(state: &EditorState) -> Paragraph<'static> {
    let buf = state.buffer();
    let cursor = buf.cursor();
    let mut lines: Vec<Line> = Vec::new();
    for (r, text) in buf.lines().iter().enumerate() {
        let toks = syntax::highlight_line(text);
        let mut spans: Vec<Span> = Vec::new();
        let mut char_idx = 0;
        let is_cursor_row = r == cursor.row;

        for tok in toks {
            let tok_chars: Vec<char> = tok.text.chars().collect();
            let tok_len = tok_chars.len();

            if is_cursor_row && char_idx <= cursor.col && cursor.col < char_idx + tok_len {
                // Split the token at the cursor position.
                let before: String = tok_chars[..cursor.col - char_idx].iter().collect();
                let at: String = tok_chars[cursor.col - char_idx..=cursor.col - char_idx]
                    .iter()
                    .collect();
                let after: String = tok_chars[cursor.col - char_idx + 1..].iter().collect();

                if !before.is_empty() {
                    spans.push(Span::styled(before, token_style(tok.kind)));
                }
                spans.push(Span::styled(
                    if at.is_empty() { " ".to_string() } else { at },
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::UNDERLINED),
                ));
                if !after.is_empty() {
                    spans.push(Span::styled(after, token_style(tok.kind)));
                }
            } else {
                spans.push(Span::styled(tok.text.clone(), token_style(tok.kind)));
            }

            char_idx += tok_len;
        }

        // Cursor at end of line: append an underscore.
        if is_cursor_row && char_idx == cursor.col {
            spans.push(Span::styled(
                "_",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White),
            ));
        }

        lines.push(Line::from(spans));
    }
    Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Editor — {}", buf.display_name())),
    )
}

/// Build the result pane widget. Shows the last N lines that fit in the pane.
pub fn render_results(state: &EditorState) -> Paragraph<'static> {
    let text = if state.results.is_empty() {
        "(no results yet — Ctrl-E evaluates the current line)".to_string()
    } else {
        state.results.join("\n")
    };
    // Count actual lines (results may contain multi-line boxed output).
    let total_lines = text.lines().count();
    // Scroll to show the bottom: ratatui clamps the offset so the last
    // lines are pinned to the widget bottom regardless of content height.
    let scroll = (total_lines as u16).saturating_sub(1);
    Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Results (gateway)"),
        )
        .scroll((scroll, 0))
}

/// Build the status bar widget.
pub fn render_status_bar(state: &EditorState) -> Paragraph<'static> {
    let buf = state.buffer();
    let left = format!(
        "{}  {}  cursor={}:{}",
        state.io_label,
        state.sec_label,
        buf.cursor().row + 1,
        buf.cursor().col + 1,
    );

    // Buffer tabs
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(left));
    spans.push(Span::raw("    "));
    for (i, buf) in state.buffers.iter().enumerate() {
        let n = i + 1;
        let name = buf
            .file()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled");
        let dirty = if buf.is_dirty() { "*" } else { "" };
        let style = if i == state.active_buffer {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(format!(" [{n}:{name}{dirty}] "), style));
    }
    spans.push(Span::raw("    "));
    spans.push(Span::raw(format!("gateway: {}", state.gateway_status)));

    Paragraph::new(Line::from(spans)).style(Style::default().fg(Color::White).bg(Color::DarkGray))
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
            .title("Menu (ESC opens/closes)"),
    )
}

/// Number of menu items (for bounds + overlay height).
pub fn menu_item_count() -> usize {
    11
}

/// Draw the whole frame. `area` is the terminal's full rect.
pub fn draw(frame: &mut ratatui::Frame, state: &EditorState) {
    use ratatui::layout::Layout;

    let palette_rows = if state.palette_expanded { 9 } else { 5 };
    
    match state.results_mode {
        1 => {
            // Mode 1: Compact - palette, editor, results (6 rows), status bar
            let chunks = Layout::vertical(layout_constraints(palette_rows)).split(frame.area());
            frame.render_widget(render_palette(state), chunks[0]);
            frame.render_widget(render_editor(state), chunks[1]);
            frame.render_widget(render_results(state), chunks[2]);
            frame.render_widget(render_status_bar(state), chunks[3]);
        }
        2 => {
            // Mode 2: Expanded results (50/50 horizontal split)
            let main_rows = Layout::vertical(vec![
                Constraint::Length(palette_rows as u16),
                Constraint::Percentage(50),
                Constraint::Percentage(50),
                Constraint::Length(1),
            ]).split(frame.area());
            
            frame.render_widget(render_palette(state), main_rows[0]);
            
            // Split the middle 50% into editor and results side by side
            let middle = Layout::horizontal(vec![
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ]).split(main_rows[1]);
            
            frame.render_widget(render_editor(state), middle[0]);
            frame.render_widget(render_results(state), middle[1]);
            frame.render_widget(render_status_bar(state), main_rows[3]);
        }
        3 => {
            // Mode 3: Split vertically (editor left, results right, 50/50)
            let main_split = Layout::horizontal(vec![
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ]).split(frame.area());
            
            // Left side: palette + editor
            let left = Layout::vertical(vec![
                Constraint::Length(palette_rows as u16),
                Constraint::Min(4),
                Constraint::Length(1),
            ]).split(main_split[0]);
            
            frame.render_widget(render_palette(state), left[0]);
            frame.render_widget(render_editor(state), left[1]);
            frame.render_widget(render_status_bar(state), left[2]);
            
            // Right side: results
            frame.render_widget(render_results(state), main_split[1]);
        }
        _ => {}
    }

    if state.menu_open {
        let menu = render_menu(state);
        let w = 28u16;
        let h = (menu_item_count() + 2) as u16;
        let area = centered_rect(w, h, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(menu, area);
    }

    // File dialog overlay (open/save).
    if let Some(dialog) = &state.dialog {
        let (w, h) = match dialog {
            Dialog::OpenFile { files, .. } => (50u16, (files.len() + 6).max(8) as u16),
            Dialog::SaveAs { .. } => (50u16, 5u16),
            Dialog::Help { scroll: _ } => (70u16, 32u16),
        };
        let area = centered_rect(w, h, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(render_dialog(dialog), area);
    }
}

/// Render the file open/save dialog overlay.
fn render_dialog(dialog: &Dialog) -> Paragraph<'static> {
    match dialog {
        Dialog::OpenFile { path, cursor, files } => {
            let mut text = String::new();
            text.push_str("Open File\n\n");
            text.push_str("Path: ");
            text.push_str(path);
            text.push_str("_\n\n");

            if files.is_empty() {
                text.push_str("(no files in directory)");
            } else {
                for (i, file) in files.iter().enumerate() {
                    if i == *cursor {
                        text.push_str("  ▶ ");
                    } else {
                        text.push_str("    ");
                    }
                    text.push_str(file);
                    text.push('\n');
                }
            }

            Paragraph::new(text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Open File (↑↓ to navigate, Enter to open, ESC to cancel)"),
            )
        }
        Dialog::SaveAs { path } => {
            let text = format!("Save As\n\n{path}_");
            Paragraph::new(text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Enter path (Enter to confirm, ESC to cancel)"),
            )
        }
        Dialog::Help { scroll } => {
            let text = "\
stride — Terminal APL Editor
═══════════════════════════════════════════════════

A full-featured TUI editor for writing and evaluating
APL expressions. Connects to interpreters (Kap, rust-apl)
via the RIDE protocol.

KEYBOARD SHORTCUTS
─────────────────
TAB             Cycle palette row (category)
Ctrl+← / Ctrl+→  Select glyph within row
Ctrl+Space      Insert focused glyph
Space           Normal space character
Enter           Newline
Backspace       Delete character before cursor
Delete          Delete character under cursor
↑ / ↓           Move cursor vertically
Ctrl+← / Ctrl→  Move cursor horizontally
Home / End      Jump to start / end of line

BUFFER MANAGEMENT
─────────────────
Ctrl+N          New buffer (cycles after 9)
Ctrl+1 … Ctrl+9 Switch to buffer N
Ctrl+W          Close current buffer
Ctrl+O          Open file into new buffer

EVALUATION
──────────
Ctrl+E          Evaluate current line
Ctrl+Enter       Execute all lines of current buffer

PALETTE
───────
Ctrl+P          Toggle expanded palette (7 rows)
Ctrl+L          Cycle results display mode (compact/expanded/split)
ESC             Open / close menu

MISCELLANEOUS
─────────────
Ctrl+S          Save current file
Ctrl+X          Close stride

ABOUT
─────
stride 0.1.0 — a terminal APL editor
RIDE-compatible gateway on port 4502
https://github.com/kapitaali/stride";

            let lines: Vec<&str> = text.lines().collect();
            let total_lines = lines.len();
            // Dialog height is 32, minus 2 for borders = 30 usable lines.
            let visible = 30usize;
            let max_scroll = total_lines.saturating_sub(visible);
            let scroll = (*scroll).min(max_scroll);
            let end = (scroll + visible).min(total_lines);
            let visible_lines = &lines[scroll..end];
            let content: String = visible_lines.join("\n");

            let title = if max_scroll > 0 {
                format!("stride — Help (↑↓ to scroll {}/{} ESC/Enter to close)", (scroll + visible).min(total_lines), total_lines)
            } else {
                "stride — Help (ESC/Enter to close)".to_string()
            };

            Paragraph::new(content).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title),
            )
        }
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
    let is_nested = v.cells().iter().any(|c| c.is_pointer_cell());
    if v.rank() >= 2 || all_chars || is_nested {
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
        s.buffers[0] = Buffer::from_string("A←⍳5\n+/A ⍝ sum");
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
        assert_eq!(s.buffer().line_count(), 2);
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
        let c = layout_constraints(5);
        assert_eq!(c.len(), 4);
        assert!(matches!(c[0], Constraint::Length(5)));
        assert!(matches!(c[3], Constraint::Length(1)));
    }

    #[test]
    fn layout_constraints_expanded() {
        let c = layout_constraints(9);
        assert_eq!(c.len(), 4);
        assert!(matches!(c[0], Constraint::Length(9)));
    }

    #[test]
    fn focused_entry_name_for_default_state() {
        let s = sample_state();
        let name = focused_entry_name(&s);
        // Default row is "Assign"; first entry is ← = assignment
        assert!(name.contains("assignment"));
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
