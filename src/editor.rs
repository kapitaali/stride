//! Text buffer: multi-line editing, char-based cursor, file I/O.
//!
//! All cursor columns count Unicode scalar values (chars), never bytes,
//! so APL glyphs are always one column.

use std::path::{Path, PathBuf};

/// Cursor position: zero-based line and char-column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
}

/// An editable multi-line text buffer with a cursor.
#[derive(Debug, Clone)]
pub struct Buffer {
    lines: Vec<String>,
    cursor: Cursor,
    file: Option<PathBuf>,
    dirty: bool,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: Cursor::default(),
            file: None,
            dirty: false,
        }
    }

    pub fn from_string(text: &str) -> Self {
        let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self {
            lines,
            cursor: Cursor::default(),
            file: None,
            dirty: false,
        }
    }

    /// Open a file (`.apl`, `.aplws`, `.xml`, anything text).
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut buf = Self::from_string(&text);
        buf.file = Some(path.to_path_buf());
        buf.dirty = false;
        Ok(buf)
    }

    /// Save back to the file this buffer was opened from / saved to.
    /// Errors when no file is associated (caller should prompt instead).
    pub fn save(&mut self) -> std::io::Result<()> {
        match self.file.clone() {
            Some(p) => self.save_as(&p),
            None => Err(std::io::Error::other("no file name (use Save As)")),
        }
    }

    pub fn save_as(&mut self, path: &Path) -> std::io::Result<()> {
        let mut text = self.lines.join("\n");
        text.push('\n');
        std::fs::write(path, text)?;
        self.file = Some(path.to_path_buf());
        self.dirty = false;
        Ok(())
    }

    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// Display name for the status bar.
    pub fn display_name(&self) -> String {
        self.file
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[untitled]".to_string())
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, row: usize) -> Option<&str> {
        self.lines.get(row).map(|s| s.as_str())
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn current_line(&self) -> &str {
        &self.lines[self.cursor.row.min(self.lines.len() - 1)]
    }

    fn line_chars(&self, row: usize) -> Vec<char> {
        self.lines[row].chars().collect()
    }

    fn clamp_cursor(&mut self) {
        self.cursor.row = self.cursor.row.min(self.lines.len() - 1);
        let len = self.line_chars(self.cursor.row).len();
        self.cursor.col = self.cursor.col.min(len);
    }

    /// Insert text at the cursor (palette glyphs, typed chars, paste).
    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let row = self.cursor.row;
        let col = self.cursor.col;
        let mut chars = self.line_chars(row);
        let byte: usize = chars.iter().take(col).map(|c| c.len_utf8()).sum();
        self.lines[row].insert_str(byte, s);
        chars = self.line_chars(row);
        let _ = chars;
        self.cursor.col = col + s.chars().count();
        self.dirty = true;
    }

    pub fn insert_char(&mut self, c: char) {
        let mut s = String::new();
        s.push(c);
        self.insert_str(&s);
    }

    /// Split the current line at the cursor (Enter key).
    pub fn insert_newline(&mut self) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        let chars = self.line_chars(row);
        let byte: usize = chars.iter().take(col).map(|c| c.len_utf8()).sum();
        let tail = self.lines[row][byte..].to_string();
        self.lines[row].truncate(byte);
        self.lines.insert(row + 1, tail);
        self.cursor.row += 1;
        self.cursor.col = 0;
        self.dirty = true;
    }

    /// Delete the char before the cursor (joins lines at column 0).
    pub fn backspace(&mut self) {
        let (row, col) = (self.cursor.row, self.cursor.col);
        if col > 0 {
            let chars = self.line_chars(row);
            let end: usize = chars.iter().take(col).map(|c| c.len_utf8()).sum();
            let start: usize = chars.iter().take(col - 1).map(|c| c.len_utf8()).sum();
            self.lines[row].replace_range(start..end, "");
            self.cursor.col -= 1;
            self.dirty = true;
        } else if row > 0 {
            let tail = self.lines.remove(row);
            self.cursor.row -= 1;
            self.cursor.col = self.line_chars(self.cursor.row).len();
            self.lines[self.cursor.row].push_str(&tail);
            self.dirty = true;
        }
    }

    /// Delete the char under the cursor (joins with next line at EOL).
    pub fn delete_forwards(&mut self) {
        let (row, col) = (self.cursor.row, self.cursor.col);
        let len = self.line_chars(row).len();
        if col < len {
            let chars = self.line_chars(row);
            let start: usize = chars.iter().take(col).map(|c| c.len_utf8()).sum();
            let end: usize = chars.iter().take(col + 1).map(|c| c.len_utf8()).sum();
            self.lines[row].replace_range(start..end, "");
            self.dirty = true;
        } else if row + 1 < self.lines.len() {
            let tail = self.lines.remove(row + 1);
            self.lines[row].push_str(&tail);
            self.dirty = true;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.row > 0 {
            self.cursor.row -= 1;
            self.cursor.col = self.line_chars(self.cursor.row).len();
        }
    }

    pub fn move_right(&mut self) {
        let len = self.line_chars(self.cursor.row).len();
        if self.cursor.col < len {
            self.cursor.col += 1;
        } else if self.cursor.row + 1 < self.lines.len() {
            self.cursor.row += 1;
            self.cursor.col = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor.row > 0 {
            self.cursor.row -= 1;
            self.clamp_cursor();
        } else {
            self.cursor.col = 0;
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor.row + 1 < self.lines.len() {
            self.cursor.row += 1;
            self.clamp_cursor();
        } else {
            self.cursor.col = self.line_chars(self.cursor.row).len();
        }
    }

    pub fn home(&mut self) {
        self.cursor.col = 0;
    }

    pub fn end(&mut self) {
        self.cursor.col = self.line_chars(self.cursor.row).len();
    }

    pub fn goto(&mut self, row: usize, col: usize) {
        self.cursor.row = row;
        self.cursor.col = col;
        self.clamp_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_apl_glyph_counts_one_column() {
        let mut b = Buffer::new();
        b.insert_str("⍳");
        b.insert_str("⎕IO");
        assert_eq!(b.current_line(), "⍳⎕IO");
        // ⍳ = 1 char, ⎕IO = 3 chars → cursor at col 4
        assert_eq!(b.cursor(), Cursor { row: 0, col: 4 });
    }

    #[test]
    fn newline_splits_line() {
        let mut b = Buffer::from_string("ABC");
        b.goto(0, 1);
        b.insert_newline();
        assert_eq!(b.lines(), &["A".to_string(), "BC".to_string()]);
        assert_eq!(b.cursor(), Cursor { row: 1, col: 0 });
    }

    #[test]
    fn backspace_joins_lines() {
        let mut b = Buffer::from_string("AB\nCD");
        b.goto(1, 0);
        b.backspace();
        assert_eq!(b.lines(), &["ABCD".to_string()]);
        assert_eq!(b.cursor(), Cursor { row: 0, col: 2 });
    }

    #[test]
    fn backspace_removes_full_glyph() {
        let mut b = Buffer::from_string("A⍳B");
        b.goto(0, 2);
        b.backspace();
        assert_eq!(b.current_line(), "AB");
    }

    #[test]
    fn vertical_moves_clamp() {
        let mut b = Buffer::from_string("ABCDEF\nXY");
        b.goto(0, 5);
        b.move_down();
        assert_eq!(b.cursor(), Cursor { row: 1, col: 2 });
        b.move_up();
        assert_eq!(b.cursor(), Cursor { row: 0, col: 2 });
    }

    #[test]
    fn save_roundtrip() {
        let p = std::env::temp_dir().join("apl_editor_buf_test.apl");
        let mut b = Buffer::from_string("A←⍳5\n+/A");
        b.save_as(&p).unwrap();
        assert!(!b.is_dirty());
        let r = Buffer::open(&p).unwrap();
        assert_eq!(r.lines(), &["A←⍳5".to_string(), "+/A".to_string()]);
        assert_eq!(r.display_name(), p.display().to_string());
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn save_without_file_errors() {
        let mut b = Buffer::new();
        assert!(b.save().is_err());
    }

    #[test]
    fn delete_forwards_at_eol_joins() {
        let mut b = Buffer::from_string("AB\nCD");
        b.goto(0, 2);
        b.delete_forwards();
        assert_eq!(b.lines(), &["ABCD".to_string()]);
    }
}
