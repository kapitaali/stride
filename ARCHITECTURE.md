# rust-apl-editor — Architecture

## Project
CLI/terminal APL editor using a Rust TUI crate (ratatui/crossterm).
Location: `/home/theb/Apps/rust-apl-editor`
Integration: connects to rust-apl interpreter via gateway (same as RIDE).

## Gateway System (RIDE-compatible)
Uses a gateway TCP socket model (same port numbers / protocol as RIDE editor).
Config file (e.g., `editor.toml`):
- `gateway_port = 4502` (same as RIDE default)
- `gateway_host = "localhost"`
- `interpreter_path = "../rust-apl/target/debug/libapl.so"`
- `apl_version = "GNU APL 2.0 (Rust)"`
- `plugin_path = "../rust-apl/target/debug/libdemo_plugin.so"`

The gateway listens on the configured port. Any tool that speaks the RIDE protocol can plug into this editor. The editor acts as a client that connects to the interpreter's gateway and sends expressions, receives results, and renders them with syntax highlighting.

## TUI Design
- Full-screen terminal UI
- Top rows: APL character palette (TAB to access top row; TAB cycles through rows)
- All APL characters supported (GNU APL 2.0 + Dyalog + Kap extensions):
  - Quad symbols (⎕A, ⎕D, ⎕IO, ⎕PP, ⎕SVx, etc.)
  - Primitive operators (⍳, ⍸, ⊂, ⊃, ⌿, ⍀, ⌽, ⍉, ⍋, ⍒, ⍋, etc.)
  - Greek letters (⍺, ⍵, ⍵⍵, ⍺⍺)
  - Box drawing characters (for output formatting)
  - Newer Dyalog extensions (⍥, ⌸, ⊥, ⊤, etc. — included even if stubbed)
- ALT key: opens menu bar above top rows (File, Edit, Help, etc.)
- Main editing area: multi-line code with syntax highlighting (primitives, variables, comments, strings)
- Status bar: current file, interpreter state, ⎕IO, ⎕SEC level
- File I/O: open, save, load workspace files (.apl / .aplws / .xml)
- Integration with interpreter: send expressions directly to `eval_line()` via plugin hooks; receive results back through the gateway

## Key Components (planned files)
- `src/main.rs` — entry point, TUI loop, event handling
- `src/editor.rs` — text editing, cursor movement, insertion of APL chars
- `src/ui.rs` — rendering the TUI (ratatui layout, top rows, menus)
- `src/syntax.rs` — syntax highlighting rules (keywords, primitives, variables)
- `src/gateway.rs` — TCP client connecting to interpreter gateway
- `src/config.rs` — load `editor.toml`, set defaults
- `src/characters.rs` — APL character database (all supported symbols)
- `ARCHITECTURE.md` — this file
- `tests/test_gateway.rs` — gateway connection and message tests
- `tests/test_editor.apl` — APL script testing editor commands
- `editor.toml` — configuration (port, interpreter, plugin paths)

## Integration with rust-apl
The editor uses the middleware hook system (`before_eval`, `before_syscmd`) to inject commands into the interpreter session. It can:
- Send `)CLEAR`, `)SAVE`, `)LOAD` via `before_syscmd`
- Monitor `⎕SEC` changes via `on_sysvar_change`
- Block dangerous expressions via `before_eval`
- Load `.apl` workspace files through `⎕LOADSO` or direct file I/O

The editor does not duplicate interpreter logic; it connects to the interpreter as a gateway client and provides a user-friendly terminal interface on top.
