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

use stride::config::EditorConfig;
use stride::editor::Buffer;
use stride::gateway::{GatewayCommand, GatewayMessage, GatewayServer};
use stride::ui::{self, Dialog, EditorState};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

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
            eprintln!("stride TUI error: {e}");
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
                    println!("{}", stride::ui::format_value_for(&v, pp));
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
                state.buffers[0] = buf;
                state.status = format!("opened {}", path.display());
            }
            Err(e) => state.status = format!("cannot open {}: {e}", path.display()),
        }
    }

    // Start the gateway server (listens for interpreters to connect).
    let (server, gateway_rx, _gateway_tx) = GatewayServer::new(state.config.gateway_port);
    let interpreter = server.interpreter();
    let server_port = server.port;
    std::thread::spawn(move || {
        let _ = server.run();
    });
    state.gateway_status = format!("listening on port {}", server_port);

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res: std::io::Result<()> = loop {
        terminal
            .draw(|f| ui::draw(f, &state))
            .map_err(|e| std::io::Error::other(format!("render: {e}")))?;

        // Check for messages from the gateway server.
        match gateway_rx.try_recv() {
            Ok(GatewayMessage::Connected { addr }) => {
                state.gateway_status = format!("connected to {addr}");
            }
            Ok(GatewayMessage::Disconnected) => {
                state.gateway_status = format!("listening on port {}", state.config.gateway_port);
            }
            Ok(GatewayMessage::Output { result, .. }) => {
                state.push_result(result);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break Ok(()),
        }

        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(key) = event::read()? {
                if state.dialog.is_some() {
                    handle_dialog_key(&mut state, key.code, key.modifiers);
                    continue;
                }
                if state.menu_open {
                    if handle_menu_key(&mut state, &interpreter, key.code) {
                        break Ok(());
                    }
                    continue;
                }
                if handle_key(&mut state, &interpreter, key.code, key.modifiers) {
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

/// Spawn the gateway executable. Returns when the process has been launched
/// (not when it's ready to accept connections).
fn spawn_gateway(config: &stride::config::EditorConfig) -> std::io::Result<()> {
    use std::process::Command;

    let exec = &config.gateway_executable;
    let args = config.gateway_args.split_whitespace().collect::<Vec<_>>();

    let mut cmd = Command::new(exec);
    cmd.args(&args)
        .arg(config.gateway_port.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    // Set environment variables (e.g., RIDE_INIT)
    for env_var in &config.gateway_env {
        if let Some((key, value)) = env_var.split_once('=') {
            cmd.env(key, value);
        }
    }

    cmd.spawn()?;

    Ok(())
}

/// Returns true when the app should quit.
fn handle_key(
    state: &mut EditorState,
    interpreter: &Arc<Mutex<Option<Sender<GatewayCommand>>>>,
    code: KeyCode,
    mods: KeyModifiers,
) -> bool {
    match (code, mods) {
        (KeyCode::Char('q'), KeyModifiers::CONTROL) => return true,
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
            eval_current_line(state, interpreter);
        }
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => match state.buffer_mut().save() {
            Ok(()) => state.status = "saved".to_string(),
            Err(e) => state.status = format!("save failed: {e}"),
        },
        (KeyCode::Char('o'), KeyModifiers::CONTROL) => {
            // Open file dialog
            state.dialog = Some(stride::ui::Dialog::OpenFile {
                path: String::new(),
                cursor: 0,
                files: list_files_for_path(""),
            });
        }
        (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
            // New buffer / cycle
            if state.buffers.len() < 9 {
                state.buffers.push(Buffer::new());
            }
            state.active_buffer = (state.active_buffer + 1) % 9;
            state.palette_col = 0;
        }
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
            // Close buffer
            if state.buffers.len() > 1 {
                state.buffers.remove(state.active_buffer);
                state.active_buffer = state.active_buffer.min(state.buffers.len() - 1);
            }
        }
        (KeyCode::Char('b'), KeyModifiers::CONTROL) => {
            // Execute all lines of current buffer
            let lines: Vec<String> = state.buffer().lines().iter().cloned().collect();
            for line in &lines {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with(')') {
                    eval_line(state, interpreter, trimmed);
                }
            }
        }
        (KeyCode::Char(c), KeyModifiers::CONTROL) if c >= '1' && c <= '9' => {
            let idx = (c as u8 - b'1') as usize;
            if idx < state.buffers.len() {
                state.active_buffer = idx;
            } else {
                while state.buffers.len() <= idx {
                    state.buffers.push(Buffer::new());
                }
                state.active_buffer = idx;
            }
        }
        // Terminal sends Ctrl+H (0x08) for Backspace on some setups; treat it as Backspace.
        (KeyCode::Char('h'), KeyModifiers::CONTROL) => state.buffer_mut().backspace(),
        (KeyCode::Tab, _) => {
            state.palette_row = (state.palette_row + 1) % stride::characters::row_count();
            state.palette_col = 0;
            state.status = stride::ui::focused_entry_name(state);
        }
        // Ctrl+P toggles expanded palette (5 rows).
        (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
            state.palette_expanded = !state.palette_expanded;
        }
        // Ctrl+L cycles results display mode (1→2→3→1).
        (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
            state.results_mode = (state.results_mode % 3) + 1;
            state.status = format!("results mode: {}", state.results_mode);
        }
        (KeyCode::BackTab, _) => {
            let n = stride::characters::row_count();
            state.palette_row = (state.palette_row + n - 1) % n;
            state.palette_col = 0;
            state.status = stride::ui::focused_entry_name(state);
        }
        (KeyCode::Right, KeyModifiers::NONE) => state.buffer_mut().move_right(),
        (KeyCode::Left, KeyModifiers::NONE) => state.buffer_mut().move_left(),
        // Ctrl+Left/Right selects a glyph in the palette row.
        (KeyCode::Right, KeyModifiers::CONTROL) => {
            let len = state.palette().entries.len();
            state.palette_col = (state.palette_col + 1) % len;
            state.status = stride::ui::focused_entry_name(state);
        }
        (KeyCode::Left, KeyModifiers::CONTROL) => {
            let len = state.palette().entries.len();
            state.palette_col = (state.palette_col + len - 1) % len;
            state.status = stride::ui::focused_entry_name(state);
        }
        (KeyCode::Enter, KeyModifiers::NONE) => state.buffer_mut().insert_newline(),
        // Ctrl+Space inserts the focused palette glyph; Space is a normal space.
        (KeyCode::Char(' '), KeyModifiers::CONTROL) => {
            let glyph = state.palette().entries[state.palette_col].glyph;
            state.buffer_mut().insert_str(glyph);
        }
        (KeyCode::Esc, _) => {
            state.menu_open = true;
            state.menu_focus = 0;
        }
        (KeyCode::Char(c), _) => {
            state.buffer_mut().insert_char(c);
        }
        (KeyCode::Backspace, _) => state.buffer_mut().backspace(),
        (KeyCode::Delete, _) => state.buffer_mut().delete_forwards(),
        (KeyCode::Up, _) => state.buffer_mut().move_up(),
        (KeyCode::Down, _) => state.buffer_mut().move_down(),
        (KeyCode::Left, _) => state.buffer_mut().move_left(),
        (KeyCode::Right, _) => state.buffer_mut().move_right(),
        (KeyCode::Home, _) => state.buffer_mut().home(),
        (KeyCode::End, _) => state.buffer_mut().end(),
        (KeyCode::Enter, _) => state.buffer_mut().insert_newline(),
        _ => {}
    }
    false
}

fn handle_menu_key(
    state: &mut EditorState,
    interpreter: &Arc<Mutex<Option<Sender<GatewayCommand>>>>,
    code: KeyCode,
) -> bool {
    match code {
        KeyCode::Esc => state.menu_open = false,
        KeyCode::Down => state.menu_focus = (state.menu_focus + 1) % stride::ui::menu_item_count(),
        KeyCode::Up => {
            let n = stride::ui::menu_item_count();
            state.menu_focus = (state.menu_focus + n - 1) % n;
        }
        KeyCode::Enter => {
            let item = state.menu_focus;
            state.menu_open = false;
            return menu_action(state, interpreter, item);
        }
        _ => {}
    }
    false
}

fn menu_action(state: &mut EditorState, interpreter: &Arc<Mutex<Option<Sender<GatewayCommand>>>>, item: usize) -> bool {
    match item {
        0 => {
            *state = EditorState::new(state.config.clone());
            state.status = "new buffer".to_string();
        }
        1 => {
            state.dialog = Some(stride::ui::Dialog::OpenFile {
                path: String::new(),
                cursor: 0,
                files: list_files_for_path(""),
            })
        }
        2 => match state.buffer_mut().save() {
            Ok(()) => state.status = "saved".to_string(),
            Err(e) => state.status = format!("save failed: {e}"),
        },
        3 => state.status = "Save As: not wired to a file dialog yet".to_string(),
        4..=8 => state.status = "edit action not yet implemented".to_string(),
        9 => {
            state.dialog = Some(Dialog::Help { scroll: 0 });
        }
        10 => return true, // Quit
        _ => {}
    }
    let _ = interpreter;
    false
}

fn eval_current_line(state: &mut EditorState, interpreter: &Arc<Mutex<Option<Sender<GatewayCommand>>>>) {
    let line = state.buffer_mut().current_line().trim().to_string();
    if line.is_empty() {
        return;
    }
    eval_line(state, interpreter, &line);
}

fn eval_line(state: &mut EditorState, interpreter: &Arc<Mutex<Option<Sender<GatewayCommand>>>>, line: &str) {
    // Intercept )system commands and handle locally
    if line.starts_with(')') {
        match apl::sysvars::syscmd(line[1..].trim(), &mut state.env) {
            None => {} // )OFF — ignore in stride
            Some(lines) => {
                for l in lines {
                    state.push_result(l);
                }
            }
        }
        return;
    }

    let (tx, rx) = channel::<String>();
    
    // Try to send to connected interpreter
    let sent = {
        let ints = interpreter.lock().unwrap();
        if let Some(ref sender) = *ints {
            sender.send(GatewayCommand::Execute {
                text: line.to_string(),
                response_tx: tx,
            }).is_ok()
        } else {
            false
        }
    };
    
    if sent {
        // Wait for response (with timeout)
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => {
                // Parse JSON response: ["AppendSessionOutput", {"result":"...", "type":0}]
                let display = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&result) {
                    if let Some(arr) = val.as_array() {
                        arr.get(1).and_then(|o| o["result"].as_str()).unwrap_or(&result).to_string()
                    } else {
                        result
                    }
                } else {
                    result
                };
                if display.is_empty() {
                    state.status = "no result (assignment)".to_string();
                } else {
                    state.push_result(format!("⎕ {display}"));
                    state.status = "evaluated via gateway".to_string();
                }
            }
            Err(_) => {
                state.push_result("ERROR: gateway timeout".to_string());
                state.status = "gateway timeout".to_string();
            }
        }
    } else {
        // No gateway: evaluate locally with the persistent interpreter.
        match state.env.eval_line(line) {
            Ok(Some(v)) => {
                let pp = apl::sysvars::get_pp(&state.env).unwrap_or(10);
                let text = stride::ui::format_value_for(&v, pp);
                state.push_result(text);
                state.status = "evaluated locally".to_string();
            }
            Ok(None) => state.status = "no result (assignment)".to_string(),
            Err(e) => {
                let rich = apl::AplError::from(e).with_source_line(line.to_string());
                state.push_result(format!("ERROR {rich}"));
                state.status = "eval error".to_string();
            }
        }
    }
}

/// Handle keys when a dialog is open.
fn handle_dialog_key(state: &mut EditorState, code: KeyCode, mods: KeyModifiers) {
    if let Some(dialog) = &mut state.dialog {
        match dialog {
            Dialog::Help { scroll } => {
                match code {
                    KeyCode::Esc | KeyCode::Enter => {
                        state.dialog = None;
                    }
                    KeyCode::Up => {
                        if *scroll > 0 {
                            *scroll -= 1;
                        }
                    }
                    KeyCode::Down => {
                        *scroll += 1;
                    }
                    _ => {}
                }
            }
            Dialog::OpenFile { path, cursor, files } => {
                match code {
                    KeyCode::Esc => {
                        state.dialog = None;
                    }
                    KeyCode::Enter => {
                        // Determine what to open/navigate
                        let selected = if !files.is_empty() && *cursor < files.len() {
                            Some(files[*cursor].clone())
                        } else {
                            None
                        };

                        // Prepend current directory to selected file name
                        let target = match &selected {
                            Some(name) => {
                                if path.ends_with('/') {
                                    format!("{path}{name}")
                                } else {
                                    format!("{path}/{name}")
                                }
                            }
                            None => path.clone(),
                        };

                        if target.is_empty() {
                            return;
                        }

                        let p = std::path::PathBuf::from(&target);
                        if p.is_dir() || target.ends_with('/') {
                            // Navigate into directory
                            let mut new_path = target;
                            if !new_path.ends_with('/') {
                                new_path.push('/');
                            }
                            *path = new_path;
                            *files = list_files_for_path(path);
                            *cursor = 0;
                        } else {
                            // Open file
                            match Buffer::open(&p) {
                                Ok(buf) => {
                                    if state.buffers.len() < 9 {
                                        state.buffers.push(buf);
                                        state.active_buffer = state.buffers.len() - 1;
                                    } else {
                                        state.buffers[state.active_buffer] = buf;
                                    }
                                    state.status = format!("opened {}", target);
                                }
                                Err(e) => state.status = format!("cannot open {}: {e}", target),
                            }
                            state.dialog = None;
                        }
                    }
                    KeyCode::Up => {
                        if *cursor > 0 {
                            *cursor -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if *cursor + 1 < files.len() {
                            *cursor += 1;
                        }
                    }
                    KeyCode::Backspace => {
                        path.pop();
                        *files = list_files_for_path(path);
                        *cursor = 0;
                    }
                    KeyCode::Char('h') if mods == KeyModifiers::CONTROL => {
                        path.pop();
                        *files = list_files_for_path(path);
                        *cursor = 0;
                    }
                    KeyCode::Char(c) => {
                        path.push(c);
                        *files = list_files_for_path(path);
                        *cursor = 0;
                    }
                    _ => {}
                }
            }
            Dialog::SaveAs { .. } => {
                state.dialog = None;
            }
        }
    }
}

/// List files in the directory specified by path.
/// If path is empty, lists current directory.
/// If path points to a directory, lists that directory.
/// Otherwise, lists the parent directory of the path.
fn list_files_for_path(path: &str) -> Vec<String> {
    use std::path::Path;

    let p = Path::new(path);
    let dir = if path.is_empty() {
        Path::new(".")
    } else if p.is_dir() {
        p
    } else if path.ends_with('/') {
        p
    } else if let Some(parent) = p.parent() {
        if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        }
    } else {
        Path::new(".")
    };

    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if name.starts_with('.') {
                    continue; // skip hidden files
                }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir {
                    files.push(format!("{}/", name));
                } else {
                    files.push(name);
                }
            }
        }
    }
    files.sort();
    files
}
