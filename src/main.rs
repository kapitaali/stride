//! APL editor TUI: entry point, event loop, gateway wiring.
//!
//! Run with `cargo run` (interactive TUI) or `apl-editor < demo.apl`
//! (pipe-driven mode, no TUI). The TUI path needs a real terminal;
//! the pipe path evaluates each line through the interpreter directly.

use std::env;
use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use rust_apl_editor::config::EditorConfig;
use rust_apl_editor::editor::Buffer;
use rust_apl_editor::gateway::GatewayClient;
use rust_apl_editor::ui::{self, EditorState};

fn main() {
    let args: Vec<String> = env::args().collect();
    let config = EditorConfig::load(&EditorConfig::default_path()).unwrap_or_default();

    // Pipe mode: stdin is not a terminal → evaluate each line, print results.
    if !io::stdin().is_terminal() {
        run_pipe_mode(&config);
        return;
    }

    // Optional: open a file passed as the first positional arg.
    let initial_file = args.get(1).map(PathBuf::from);
    match run_tui_mode(config, initial_file) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("apl-editor TUI error: {e}");
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Pipe mode
// ---------------------------------------------------------------------------

fn run_pipe_mode(config: &EditorConfig) {
    let mut env = apl::parser::Environment::new();
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).unwrap_or_default();
    for line in buf.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match env.eval_line(trimmed) {
            Ok(Some(v)) => {
                let pp = apl::sysvars::get_pp(&env).unwrap_or(10);
                let all_chars =
                    !v.cells().is_empty() && v.cells().iter().all(|c| c.is_character_cell());
                if v.rank() >= 2 || all_chars {
                    for l in apl::boxdisplay::render_plain_with_pp(&v, pp) {
                        println!("{l}");
                    }
                } else {
                    println!("{}", rust_apl_editor::ui::format_value_for(&v, pp));
                }
            }
            Ok(None) => {}
            Err(e) => {
                let rich = apl::AplError::from(e).with_source_line(trimmed.to_string());
                eprintln!("ERROR: {rich}");
            }
        }
    }
    let _ = config;
}

// ---------------------------------------------------------------------------
// TUI mode
// ---------------------------------------------------------------------------

fn run_tui_mode(config: EditorConfig, initial_file: Option<PathBuf>) -> std::io::Result<()> {
    let mut state = EditorState::new(config);

    if let Some(path) = initial_file {
        match Buffer::open(&path) {
            Ok(buf) => {
                state.buffer = buf;
                state.status = format!("opened {}", path.display());
            }
            Err(e) => state.status = format!("cannot open {}: {e}", path.display()),
        }
    }

    // Try an early gateway connection (non-fatal).
    let mut gateway: Option<GatewayClient> = None;
    if state.config.auto_connect {
        match GatewayClient::connect(&state.config.gateway_host, state.config.gateway_port) {
            Ok(mut c) => {
                match c.handshake() {
                    Ok(()) => {
                        state.gateway_status = format!("connected to {}", c.addr());
                        gateway = Some(c);
                    }
                    Err(e) => state.gateway_status = format!("handshake failed: {e}"),
                }
            }
            Err(e) => state.gateway_status = format!("connect failed: {e}"),
        }
    }

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res: std::io::Result<()> = loop {
        terminal
            .draw(|f| ui::draw(f, &state))
            .map_err(|e| std::io::Error::other(format!("render: {e}")))?;

        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(key) = event::read()? {
                if state.menu_open {
                    if handle_menu_key(&mut state, &mut gateway, key.code) {
                        break Ok(());
                    }
                    continue;
                }
                if handle_key(&mut state, &mut gateway, key.code, key.modifiers) {
                    break Ok(());
                }
            }
        }
    };

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    res
}

/// Returns true when the app should quit.
fn handle_key(
    state: &mut EditorState,
    gateway: &mut Option<GatewayClient>,
    code: KeyCode,
    mods: KeyModifiers,
) -> bool {
    match (code, mods) {
        (KeyCode::Char('q'), KeyModifiers::CONTROL) => return true,
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
            eval_current_line(state, gateway);
        }
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => match state.buffer.save() {
            Ok(()) => state.status = "saved".to_string(),
            Err(e) => state.status = format!("save failed: {e}"),
        },
        // Terminal sends Ctrl+H (0x08) for Backspace on some setups; treat it as Backspace.
        (KeyCode::Char('h'), KeyModifiers::CONTROL) => state.buffer.backspace(),
        (KeyCode::Tab, _) => {
            state.palette_row = (state.palette_row + 1) % rust_apl_editor::characters::row_count();
            state.palette_col = 0;
            state.status = rust_apl_editor::ui::focused_entry_name(state);
        }
        (KeyCode::BackTab, _) => {
            let n = rust_apl_editor::characters::row_count();
            state.palette_row = (state.palette_row + n - 1) % n;
            state.palette_col = 0;
            state.status = rust_apl_editor::ui::focused_entry_name(state);
        }
        (KeyCode::Right, KeyModifiers::NONE) => state.buffer.move_right(),
        (KeyCode::Left, KeyModifiers::NONE) => state.buffer.move_left(),
        // Ctrl+Left/Right selects a glyph in the palette row.
        (KeyCode::Right, KeyModifiers::CONTROL) => {
            let len = state.palette().entries.len();
            state.palette_col = (state.palette_col + 1) % len;
            state.status = rust_apl_editor::ui::focused_entry_name(state);
        }
        (KeyCode::Left, KeyModifiers::CONTROL) => {
            let len = state.palette().entries.len();
            state.palette_col = (state.palette_col + len - 1) % len;
            state.status = rust_apl_editor::ui::focused_entry_name(state);
        }
        (KeyCode::Enter, KeyModifiers::NONE) => state.buffer.insert_newline(),
        // Ctrl+Space inserts the focused palette glyph; Space is a normal space.
        (KeyCode::Char(' '), KeyModifiers::CONTROL) => {
            let glyph = state.palette().entries[state.palette_col].glyph;
            state.buffer.insert_str(glyph);
        }
        (KeyCode::Esc, _) => {
            state.menu_open = true;
            state.menu_focus = 0;
        }
        (KeyCode::Char(c), _) => {
            state.buffer.insert_char(c);
        }
        (KeyCode::Backspace, _) => state.buffer.backspace(),
        (KeyCode::Delete, _) => state.buffer.delete_forwards(),
        (KeyCode::Up, _) => state.buffer.move_up(),
        (KeyCode::Down, _) => state.buffer.move_down(),
        (KeyCode::Left, _) => state.buffer.move_left(),
        (KeyCode::Right, _) => state.buffer.move_right(),
        (KeyCode::Home, _) => state.buffer.home(),
        (KeyCode::End, _) => state.buffer.end(),
        (KeyCode::Enter, _) => state.buffer.insert_newline(),
        _ => {}
    }
    false
}

fn handle_menu_key(
    state: &mut EditorState,
    gateway: &mut Option<GatewayClient>,
    code: KeyCode,
) -> bool {
    match code {
        KeyCode::Esc => state.menu_open = false,
        KeyCode::Down => state.menu_focus = (state.menu_focus + 1) % rust_apl_editor::ui::menu_item_count(),
        KeyCode::Up => {
            let n = rust_apl_editor::ui::menu_item_count();
            state.menu_focus = (state.menu_focus + n - 1) % n;
        }
        KeyCode::Enter => {
            let item = state.menu_focus;
            state.menu_open = false;
            return menu_action(state, gateway, item);
        }
        _ => {}
    }
    false
}

fn menu_action(state: &mut EditorState, gateway: &mut Option<GatewayClient>, item: usize) -> bool {
    match item {
        0 => {
            *state = EditorState::new(state.config.clone());
            state.status = "new buffer".to_string();
        }
        1 => state.status = "Open: not wired to a file dialog yet".to_string(),
        2 => match state.buffer.save() {
            Ok(()) => state.status = "saved".to_string(),
            Err(e) => state.status = format!("save failed: {e}"),
        },
        3 => state.status = "Save As: not wired to a file dialog yet".to_string(),
        4..=8 => state.status = "edit action not yet implemented".to_string(),
        9 => {
            state.status = format!(
                "apl-editor {} — {}",
                env!("CARGO_PKG_VERSION"),
                state.config.apl_version
            )
        }
        10 => return true, // Quit
        _ => {}
    }
    let _ = gateway;
    false
}

fn eval_current_line(state: &mut EditorState, gateway: &mut Option<GatewayClient>) {
    let line = state.buffer.current_line().trim().to_string();
    if line.is_empty() {
        return;
    }
    if let Some(gw) = gateway {
        match gw.eval(&line) {
            Ok(result) => {
                state.push_result(format!("⎕ {result}"));
                state.status = "evaluated via gateway".to_string();
            }
            Err(msg) => {
                state.push_result(format!("ERROR {msg}"));
                state.status = "gateway eval failed".to_string();
            }
        }
    } else {
        // No gateway: evaluate locally with the interpreter.
        let mut env = apl::parser::Environment::new();
        match env.eval_line(&line) {
            Ok(Some(v)) => {
                let pp = apl::sysvars::get_pp(&env).unwrap_or(10);
                let text = rust_apl_editor::ui::format_value_for(&v, pp);
                state.push_result(text);
                state.status = "evaluated locally".to_string();
            }
            Ok(None) => state.status = "no result (assignment)".to_string(),
            Err(e) => {
                let rich = apl::AplError::from(e).with_source_line(line);
                state.push_result(format!("ERROR {rich}"));
                state.status = "eval error".to_string();
            }
        }
    }
}
