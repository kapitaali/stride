# HANDOFF PROMPT — rust-apl-editor

## Context
This is a separate CLI/terminal APL editor project under ~/Apps/rust-apl-editor/. It is related to the rust-apl interpreter (~/Apps/rust-apl/) but is a standalone TUI tool.

## Current State
- ARCHITECTURE.md defines the design: Rust TUI (ratatui/crossterm), RIDE-compatible gateway (port 4502), TAB-accessible APL character palette, ALT menus, syntax highlighting, file I/O.
- Plugin middleware hooks (before_eval, before_syscmd, on_sysvar_change) have been added to rust-apl's plugin system. The editor can register its own SecurityPlugin-style hooks.
- No source code has been written yet (only ARCHITECTURE.md and NOTE_MOVE.md).

## Roadmap Integration Status (rust-apl main project — all complete)
- Phases 1-11 fully implemented (last commit: 98a54d2, 777 passing tests, 782 with python, 811 with ipc)
- ⎕FIO: all 10 functions work (read/write line/bytes, open/close, seek, file size, list)
- ⎕CALL: native function dispatch working
- ⎕SVx: in-process registry (offer/query/read/set/cancel)
- )CONTINUE: saves CONTINUE.xml
- ⎕SEC: security extension via middleware hooks (3 levels: 0/1/2)
- Plugin hooks: before_eval, before_syscmd, on_sysvar_change (working with 5 LOADSO tests)
- ⎕PLOT: bar charts + line plots (matrix series) with legend
- ⎕PYTHON: pyo3 in-process + shell-out fallback (optional feature)
- Phase 10 (extra GNU APL vars): ⎕A, ⎕D, ⎕PW, ⎕LX, ⎕EM, ⎕EC, ⎕WI all done
- Phase 11 (IPC): TCP server/client/protocol implemented (22 tests, 782 passing with python feature)
- ASCII plot: expanded with bar charts and line plots

## What the Editor Needs
1. Initialize as a Rust binary crate with TUI dependencies (ratatui, crossterm)
2. Read ARCHITECTURE.md and implement components:
   - src/main.rs — REPL loop + TUI event loop
   - src/editor.rs — file editing, cursor, APL char insertion
   - src/ui.rs — TUI rendering (ratatui layout, palette rows, menus)
   - src/syntax.rs — syntax highlighting rules
   - src/gateway.rs — TCP client connecting to interpreter at port 4502
   - src/config.rs — editor.toml parser
   - src/characters.rs — full APL char database (GNU + Dyalog + Kap)
3. The gateway connects to rust-apl interpreter; expressions evaluated through interpreter's middleware hooks
4. Demonstrate TAB key cycles through APL character palette rows
5. Demonstrate ALT key shows File/Edit/Help menus above palette
6. Must work with both `cargo run` and as `apl < demo.apl` interactive mode

## Integration Points with rust-apl
- Use `init_plugins()` (with hooks parameter) to register editor hooks
- Use `before_eval` to inject or monitor expressions
- Use `before_syscmd` to handle ) commands
- Use `before_eval` to enforce ⎕SEC security levels
- Gateway communicates via TCP on port 4502 (same protocol as RIDE editor, compatible with AP210 shared variable server from Phase 11)

## Key References
- ARCHITECTURE.md (this directory)
- ~/Apps/rust-apl/src/plugin_system/mod.rs (AplPluginHooks trait)
- ~/Apps/rust-apl/src/parser.rs (Expression enum, before_eval integration)
- ~/Apps/rust-apl/src/sysvars.rs (syscmd hook integration)
- ~/Apps/rust-apl/src/plugins/security.rs (⎕SEC security extension via hooks)
- ~/Apps/rust-apl/src/quad.rs (all ⎕ functions including ⎕FIO, ⎕SVx, ⎕APLOT)
- ~/Apps/rust-apl/src/ipc/ (TCP server/client/protocol for IPC)
- ~/Apps/rust-apl/META-INF/ROADMAP.md (full phase status)
- ~/Apps/rust-apl/META-INF/PROGRESS-20260905.md (session logs with verification results)

## Verification Goal
The editor should be able to:
- Open a file from ~/Apps/rust-apl/examples/
- Display APL code with syntax highlighting
- Send expressions to the interpreter via gateway
- Receive results back (with security enforcement from ⎕SEC plugin active)
- Handle file I/O commands safely (blocked at ⎕SEC ≥ 2 by plugin hooks)
- Show APL character palette accessible via TAB
- Show menus accessible via ALT

Start implementing from ARCHITECTURE.md. All source references in ~/Apps/rust-apl/ are available.
