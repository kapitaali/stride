# Multi-Buffer Support Plan

## User Experience

### Opening and Switching Buffers

| Key | Action |
|-----|--------|
| **Ctrl + N** | Open a new buffer. If 9 buffers already exist, cycle to buffer 1. |
| **Ctrl + 1** – **Ctrl + 9** | Switch to buffer N. If that buffer doesn't exist, create it. |
| **Ctrl + W** | Close the current buffer (switches to previous, or buffer 1 if last one). |
| **Ctrl + O** | Open a file into a **new** buffer. Prompts for path. |
| **Ctrl + B** | Execute **all lines** of the current buffer (batch evaluate). |

### Visual Feedback

The status bar shows buffer tabs:

```
⎕IO=1  ⎕SEC=0  cursor=1:1    [1:untitled*] [2:calc.apl] [3:]    gateway: connected
```

- Active buffer is highlighted (bold/white)
- Inactive buffers are dimmed (dark gray)
- Dirty buffers (unsaved changes) marked with `*`
- Empty slots shown as `[N:]` or omitted entirely

### Behavior

- Each buffer has its own: cursor position, lines, file association, dirty state
- Results pane is **global** (shared across all buffers) — shows output from whichever buffer was evaluated
- Ctrl+S saves the **current** buffer
- Ctrl+E evaluates the current line of the **current** buffer
- When the last buffer is closed, a new untitled buffer is created automatically (always at least 1)

### Workflow Example

1. User opens editor → buffer 1 (untitled)
2. Types some APL, presses Ctrl-N → buffer 2 (untitled)
3. Opens a file → loaded into buffer 2
4. Ctrl-1 → back to buffer 1
5. Ctrl-N → buffer 3 (untitled)
6. ... up to buffer 9
7. Ctrl-N again → cycles to buffer 1

## Technical Design

### Data Structures

```rust
pub struct EditorState {
    pub buffers: Vec<Buffer>,        // Max 9
    pub active_buffer: usize,        // Index into buffers (0-based)
    // ... other fields unchanged
}
```

### Buffer Indexing

- Internal: 0-based (`active_buffer: 0..9`)
- User-facing: 1-based (Ctrl-1 maps to index 0)
- `Ctrl-N` logic:
  ```rust
  if buffers.len() < 9 {
      buffers.push(Buffer::new());
      active_buffer = buffers.len() - 1;
  } else {
      active_buffer = (active_buffer + 1) % 9;  // cycle
  }
  ```

### Keybinding Handling

```rust
// Ctrl+N: new buffer / cycle
(KeyCode::Char('n'), KeyModifiers::CONTROL) => {
    if state.buffers.len() < 9 {
        state.buffers.push(Buffer::new());
    }
    state.active_buffer = (state.buffers.len()) % 9;
    state.palette_col = 0;
}

// Ctrl+1..9: switch to buffer
(KeyCode::Char(c), KeyModifiers::CONTROL) if c >= '1' && c <= '9' => {
    let idx = (c as u8 - b'1') as usize;
    if idx < state.buffers.len() {
        state.active_buffer = idx;
    } else {
        // Create buffers up to and including idx
        while state.buffers.len() <= idx {
            state.buffers.push(Buffer::new());
        }
        state.active_buffer = idx;
    }
}

// Ctrl+W: close buffer
(KeyCode::Char('w'), KeyModifiers::CONTROL) => {
    if state.buffers.len() > 1 {
        state.buffers.remove(state.active_buffer);
        state.active_buffer = state.active_buffer.min(state.buffers.len() - 1);
    }
}

// Ctrl+O: open file into new buffer
(KeyCode::Char('o'), KeyModifiers::CONTROL) => {
    // Prompt for file path (simple input dialog)
    // If path provided, open into new buffer
    // If 9 buffers exist, cycle to buffer 1 first
    if state.buffers.len() < 9 {
        state.buffers.push(Buffer::new());
        state.active_buffer = state.buffers.len() - 1;
    } else {
        state.active_buffer = (state.active_buffer + 1) % 9;
    }
    // Load file into the active buffer
    // state.buffer_mut().open(&path)
}

// Ctrl+B: execute all lines of current buffer
(KeyCode::Char('b'), KeyModifiers::CONTROL) => {
    let buffer = state.buffer();
    for line in buffer.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with(')') {
            eval_line(state, gateway, trimmed);
        }
    }
}
```

### Status Bar Rendering

```rust
fn render_status_bar(state: &EditorState) -> Paragraph<'static> {
    let mut spans = vec![];
    
    // Buffer tabs
    for (i, buf) in state.buffers.iter().enumerate() {
        let n = i + 1;
        let name = if buf.file().is_some() {
            buf.file().unwrap().file_name().unwrap().to_str().unwrap()
        } else {
            "untitled"
        };
        let dirty = if buf.is_dirty() { "*" } else { "" };
        let style = if i == state.active_buffer {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(format!(" [{n}:{name}{dirty}] "), style));
    }
    
    // ... rest of status bar
}
```

### Migration

Current `EditorState` has `pub buffer: Buffer`. This becomes:

```rust
// Before
pub buffer: Buffer,

// After  
pub buffers: Vec<Buffer>,
pub active_buffer: usize,
```

All existing code that reads `state.buffer` becomes `&state.buffers[state.active_buffer]`.

A helper method simplifies this:

```rust
impl EditorState {
    pub fn buffer(&self) -> &Buffer {
        &self.buffers[self.active_buffer]
    }
    
    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.active_buffer]
    }
}
```

### Files to Modify

| File | Change |
|------|--------|
| `src/ui.rs` | Add `buffers: Vec<Buffer>`, `active_buffer: usize` to `EditorState`. Add buffer tab rendering to status bar. |
| `src/main.rs` | Add Ctrl+N, Ctrl+1..9, Ctrl+W, Ctrl+O, Ctrl+B keybindings. Update all `state.buffer` → `state.buffer_mut()`. Add file path input dialog for Ctrl+O. |
| `src/editor.rs` | No changes needed (Buffer struct unchanged). |
| `README.md` | Document new keybindings. |

### Testing

- Test creating buffers up to 9
- Test cycling past 9
- Test Ctrl+1..9 switching
- Test Ctrl+W closing
- Test dirty state per buffer
- Test file association per buffer
- Test cursor position preserved when switching

## Open Questions

1. **Should results be per-buffer or global?** 
   - Global is simpler, matches current behavior
   - Per-buffer would let users see output from each file separately
   - Recommendation: global for now, can add later

2. **Should Ctrl+N cycle or stop at 9?**
   - User specified: cycle back to buffer 1
   - Alternative: do nothing when at 9
   - Recommendation: follow user spec (cycle)

3. **Should closing a dirty buffer prompt to save?**
   - Yes, like most editors
   - Recommendation: prompt "Save changes? (y/n/cancel)"

4. **Should there be a buffer list in the menu?**
   - Could add "Buffers › List" to ESC menu
   - Recommendation: add later if needed
