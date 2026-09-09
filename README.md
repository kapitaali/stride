# stride

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
cd ~/Apps/stride
cargo build --release
```

Requires the sibling `rust-apl` interpreter at `../rust-apl/` (the gateway client links against its IPC protocol types).

## Running

```bash
# Interactive TUI (auto-starts the gateway if not running)
cargo run

# Open a file directly
cargo run -- ~/Apps/rust-apl/examples/calc-demo.apl

# Pipe mode (evaluate a script, print results)
cargo run --quiet < demo.apl

# Run with a different port (useful for multiple instances)
cargo run -- --port 4503
```

The editor will automatically start the gateway executable if it's not already running on the configured port. You can also start it manually:

```bash
# Start the interpreter server (in a separate terminal)
cd ../rust-apl && cargo run -- --serve 4502
```

## Controls

| Key | Action |
|-----|--------|
| **TAB** | Cycle palette row (category) |
| **Ctrl + P** | Toggle expanded palette (5 rows) |
| **Ctrl + L** | Cycle results display mode (compact/expanded/split) |
| **Left / Right** | Move text cursor horizontally |
| **Up / Down** | Move cursor vertically |
| **Ctrl + Left / Right** | Select glyph within the palette row |
| **Ctrl + Space** | Insert the focused glyph at the cursor |
| **Space** | Normal space character |
| **Enter** | Newline |
| **Backspace** | Delete character before cursor |
| **Delete** | Delete character under cursor |
| **Home / End** | Jump to start / end of line |
| **Ctrl + E** | Evaluate the current line |
| **Ctrl + B** | Execute all lines of current buffer |
| **Ctrl + S** | Save the current file |
| **Ctrl + O** | Open file into new buffer |
| **Ctrl + N** | New buffer (cycles after 9) |
| **Ctrl + 1** – **Ctrl + 9** | Switch to buffer N |
| **Ctrl + W** | Close current buffer |
| **ESC** | Open / close the menu |
| **Ctrl + Q** | Quit |

## Configuration

`editor.toml` in the working directory:

```toml
gateway_host = "127.0.0.1"
gateway_port = 4502
gateway_executable = "../rust-apl/target/debug/apl"
gateway_args = "--serve"
gateway_env = []
apl_version = "GNU APL 2.0 (Rust)"
auto_connect = true
max_results = 500
```

| Field | Default | Description |
|-------|---------|-------------|
| `gateway_host` | `127.0.0.1` | Host of the interpreter gateway |
| `gateway_port` | `4502` | Port of the interpreter gateway (RIDE default) |
| `gateway_executable` | `../rust-apl/target/debug/apl` | Path to the interpreter executable |
| `gateway_args` | `--serve` | Arguments passed to the gateway executable |
| `gateway_env` | `[]` | Environment variables to set when spawning the gateway (e.g., `RIDE_INIT=CONNECT:localhost:4502` for Kap) |
| `apl_version` | `GNU APL 2.0 (Rust)` | Version string shown in the status bar |
| `auto_connect` | `true` | Connect to the gateway automatically at startup |
| `max_results` | `500` | Maximum result-pane lines kept in memory |

The editor will automatically start `gateway_executable` if it's not already running on the configured port. You can also start it manually with `cargo run -- --serve 4502`.

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
