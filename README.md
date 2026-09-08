# rust-apl-editor

A terminal-based APL editor with a full TUI, built in Rust. Write APL code with syntax highlighting, browse a searchable character palette, and evaluate expressions through a RIDE-compatible gateway to the [rust-apl](https://github.com/nousresearch/rust-apl) interpreter.

## Features

- **Full TUI** — ratatui/crossterm, four-pane layout (palette / editor / results / status)
- **APL character palette** — 95 glyphs in 11 logical rows, TAB to cycle, arrows to select, Space to insert
- **Syntax highlighting** — per-token coloring for primitives, operators, quad names, strings, comments, numbers, syscommands, and dfns
- **Multi-line editor** — char-based cursor (APL glyphs count as one column), file open/save, dirty tracking
- **RIDE-compatible gateway** — TCP client on port 4502, reusing `apl::ipc::protocol` types
- **Pipe mode** — `apl-editor < demo.apl` evaluates a script without the TUI
- **ESC menu** — File / Edit / Help / QUIT overlay

## Building

```bash
cd ~/Apps/rust-apl-editor
cargo build --release
```

Requires the sibling `rust-apl` interpreter at `../rust-apl/` (the gateway client links against its IPC protocol types).

## Running

```bash
# Start the interpreter server (in a separate terminal)
cd ../rust-apl && cargo run -- --serve 4502

# Interactive TUI
cargo run

# Open a file directly
cargo run -- ~/Apps/rust-apl/examples/calc-demo.apl

# Pipe mode (evaluate a script, print results)
cargo run --quiet < demo.apl
```

## Controls

| Key | Action |
|-----|--------|
| **TAB** | Cycle palette row (category) |
| **Ctrl + P** | Toggle expanded palette (5 rows) |
| **Left / Right** | Move text cursor horizontally |
| **Ctrl + Left / Right** | Select glyph within the palette row |
| **Ctrl + Space** | Insert the focused glyph at the cursor |
| **Space** | Normal space character |
| **Enter** | Newline |
| **Backspace** | Delete character before cursor |
| **Delete** | Delete character under cursor |
| **Up / Down** | Move cursor vertically |
| **Ctrl + Left / Right** | Move cursor horizontally |
| **Home / End** | Jump to start / end of line |
| **Ctrl + E** | Evaluate the current line |
| **Ctrl + S** | Save the current file |
| **ESC** | Open / close the menu |
| **Ctrl + Q** | Quit |

## Configuration

`editor.toml` in the working directory:

```toml
gateway_host = "127.0.0.1"
gateway_port = 4502
interpreter_path = "../rust-apl/target/debug/libapl.so"
apl_version = "GNU APL 2.0 (Rust)"
plugin_path = "../rust-apl/target/debug/libdemo_plugin.so"
auto_connect = true
max_results = 500
```

Missing fields fall back to RIDE-compatible defaults. A missing file is not an error — you get the defaults.

## Architecture

```
src/
├── main.rs        Entry point, TUI event loop, pipe mode
├── lib.rs         Crate root, module declarations
├── characters.rs  APL glyph database (95 glyphs, 11 rows)
├── config.rs      editor.toml parser with defaults
├── editor.rs      Multi-line buffer, char-based cursor, file I/O
├── gateway.rs     TCP client to interpreter gateway (port 4502)
├── syntax.rs      Per-line tokenizer + token classification
└── ui.rs          ratatui layout, palette, menus, status bar
```

### Data flow

1. User types or inserts a glyph → `Buffer` updates its line data + cursor
2. `ui.rs` reads `EditorState` each frame and renders four panes
3. On Ctrl+E, the current line is sent to the gateway (or evaluated locally if disconnected)
4. Results are pushed into `EditorState.results` and displayed in the result pane

### Palette rows

| Row | Category | Example glyphs |
|-----|----------|---------------|
| 1 | Assign / Struct | ← ⇐ ⟦ ⟧ |
| 2 | Arithmetic | + − × ÷ * ⍟ √ ⌹ ○ ! ? |
| 3 | Magnitude / Encode | \| ⌈ ⌊ ⊥ ⊤ ⊣ ⊢ ⌸ |
| 4 | Compare | = ≠ ≤ < > ≥ ≡ ≢ |
| 5 | Logical | ∨ ∧ ⍲ ⍱ |
| 6 | Structural | ↑ ↓ ⊂ ⊃ ⊆ ⊇ ⌷ ⍋ ⍒ ≬ ⫇ |
| 7 | Membership / Index | ⍳ ⍸ ∊ ⍷ ∪ ∩ ~ / \ ⌿ ⍀ … |
| 8 | Catenate / Reshape | , ⍪ ⍮ ⍴ ⌽ ⊖ ⍉ |
| 9 | Operators | ¨ ⍨ ⍣ ∙ ⌻ ˝ ∘ ⍛ ⍤ ⍥ ⍢ ⍫ ∵ ∥ λ ⍞ ⍎ ⍕ ⍰ |
| 10 | Punctuation / Greek | « » ⋄ ⍝ → ⍵ ⍺ ∇ ⍓ |
| 11 | Misc | ¯ ⍬ ∆ ⍙ |

## Integration with rust-apl

The editor connects to the rust-apl interpreter as a gateway client on port 4502 using the **RIDE binary-framed protocol** (same protocol as the [RIDE editor](https://github.com/Dyalog/ride)):

```
Framing: [4 bytes BE length][4 bytes "RIDE"][JSON payload]
Commands: ["Execute", {"text": "2+2"}]
Responses: ["AppendSessionOutput", {"result": "4"}]
```

Start the interpreter server first:

```bash
cd ../rust-apl && cargo run -- --serve 4502
```

The editor auto-connects on startup and performs the handshake:
1. `SupportedProtocols=2` → `UsingProtocol=2`
2. `["Identify", ...]` → `["ReplyIdentify", ...]`
3. `["Connect", ...]` → `["ReplyConnect", ...]`

Ctrl+E sends the current line to the server for evaluation. When the gateway is disconnected, Ctrl+E falls back to evaluating locally with `Environment::eval_line()`.

## Testing

```bash
cargo test
```

34 unit tests cover the palette database, config parsing, buffer editing, syntax highlighting, gateway response parsing, and UI widget construction.

## License

MIT
