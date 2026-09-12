# stride

A terminal-based APL editor with a full TUI, built in Rust. Write APL code with syntax highlighting, browse a searchable character palette, and evaluate expressions through a RIDE-compatible gateway to a [rust-apl](https://github.com/kapitaali/rust-apl) (or any RIDE-speaking) interpreter.

## Features

- **Full TUI** — ratatui/crossterm, four-pane layout (palette / editor / results / status)
- **APL character palette** — 95 glyphs in 11 logical rows, TAB to cycle, arrows to select, Space to insert
- **Syntax highlighting** — per-token coloring for primitives, operators, quad names, strings, comments, numbers, syscommands, and dfns
- **Multi-line editor** — char-based cursor (APL glyphs count as one column), file open/save, dirty tracking
- **RIDE-compatible gateway server** — interpreters (rust-apl, Kap) connect to stride on port 4502 and evaluate over the RIDE protocol
- **Three results layouts** — Ctrl+L cycles compact, expanded, and side-by-side panes
- **ESC menu** — File / Edit / Help / QUIT overlay

## Building

```bash
cd ~/Apps/stride
cargo build --release
```

Stride has no interpreter dependency at build time: it speaks the RIDE protocol over TCP and connects to whatever interpreter you point it at at runtime (the shipped `editor.toml` points at the sibling `../rust-apl/` checkout).

## Running

```bash
# Interactive TUI (with auto_connect = true, the interpreter is
# started for you and connects back to stride)
cargo run

# Open a file directly
cargo run -- ~/Apps/rust-apl/examples/calc-demo.apl

# Run with a different port (useful for multiple instances)
cargo run -- --port 4503
```

Stride listens on the configured port; interpreters connect to it. To start the interpreter yourself instead of letting `auto_connect` do it:

```bash
# In a separate terminal (rust-apl as a RIDE client of stride)
RIDE_INIT=CONNECT:127.0.0.1:4502 ../rust-apl/target/debug/apl --ride
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
| **Ctrl + R** | Execute all lines of the current buffer |
| **Ctrl + S** | Save the current file |
| **Ctrl + O** | Open file into new buffer |
| **Ctrl + N** | New buffer (cycles after 9) |
| **Ctrl + 1** – **Ctrl + 9** | Switch to buffer N |
| **Ctrl + W** | Close current buffer |
| **ESC** | Open / close the menu |
| **Ctrl + X** | Quit |

**Ctrl + Enter** also executes the whole buffer, but only in terminals that support the enhanced keyboard protocol (kitty, foot, wezterm, ghostty). Plain terminals send Ctrl+Enter as an ordinary Enter, which is why Ctrl + R is the portable binding; stride requests the protocol on startup and releases it on exit.

## Configuration

`editor.toml` (looked up in the working directory, next to the executable, or in `~/.config/stride/`):

```toml
gateway_host = "127.0.0.1"
gateway_port = 4502
gateway_executable = "../rust-apl/target/debug/apl"
gateway_args = "--ride"
gateway_env = ["RIDE_INIT=CONNECT:127.0.0.1:4502"]
apl_version = "GNU APL 2.0 (Rust)"
auto_connect = true
max_results = 500
```

| Field | Default | Description |
|-------|---------|-------------|
| `gateway_host` | `127.0.0.1` | Host the gateway listens on (and interpreters connect to) |
| `gateway_port` | `4502` | Port of the interpreter gateway (RIDE default) |
| `gateway_executable` | `../rust-apl/target/debug/apl` | Path to the interpreter executable to spawn |
| `gateway_args` | `--serve` | Arguments for the interpreter; use `--ride` for RIDE client mode (as above) |
| `gateway_env` | `[]` | Environment for the spawned interpreter; RIDE mode needs `RIDE_INIT=CONNECT:<host>:<port>` |
| `apl_version` | `GNU APL 2.0 (Rust)` | Version string shown in the status bar |
| `auto_connect` | `true` | Spawn the interpreter automatically at startup |
| `max_results` | `500` | Maximum result-pane lines kept in memory |

Missing fields fall back to the defaults above; a missing file is not an error — you get the defaults.

## Architecture

```
src/
├── main.rs        Entry point, TUI event loop, key handling
├── lib.rs         Crate root, module declarations
├── characters.rs  APL glyph database (95 glyphs, 11 rows)
├── config.rs      editor.toml parser with defaults
├── editor.rs      Multi-line buffer, char-based cursor, file I/O
├── gateway.rs     RIDE protocol server (handshake, framing, Execute round trips)
├── syntax.rs      Per-line tokenizer + token classification
└── ui.rs          ratatui layout, palette, menus, status bar
```

### Data flow

1. User types or inserts a glyph → `Buffer` updates its line data + cursor
2. `ui.rs` reads `EditorState` each frame and renders four panes
3. On Ctrl+E (current line) or Ctrl+R (whole buffer), lines are sent to the connected interpreter via the gateway
4. Results arrive as `AppendSessionOutput` messages and are pushed into `EditorState.results` for the result pane

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

## RIDE protocol

Interpreters connect to stride's gateway server on port 4502 and speak the **RIDE binary-framed protocol** (same protocol as the [RIDE editor](https://github.com/Dyalog/ride)):

```
Framing: [4 bytes BE length][4 bytes "RIDE"][JSON payload]
Commands: ["Execute", {"text": "2+2"}]
Responses: ["AppendSessionOutput", {"result": "4", "type": 2}]
```

Handshake on connect:
1. Interpreter sends `SupportedProtocols=2` → stride answers `UsingProtocol=2` (mirroring the client's framing, so strictly framed interpreters like Kap and plain-text clients like rust-apl both work)
2. Interpreter sends `["Identify", ...]` → stride replies `["ReplyIdentify", ...]`
3. Interpreter sends `["Connect", ...]` → stride replies `["ReplyConnect", ...]`

System commands (`)HELP`, `]BOXING`, …) typed in the editor run in the interpreter and come back as session output type 4. If no interpreter is connected, evaluations are reported as such in the results pane until one connects.

## Testing

```bash
cargo test
```

35 unit tests cover the palette database, config parsing, buffer editing, syntax highlighting, gateway framing/parsing, results-pane rendering, and UI widget construction.

## License

MIT
