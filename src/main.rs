//! APL editor TUI: entry point, event loop, gateway wiring.

use std::env;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use stride::config::EditorConfig;
use stride::editor::Buffer;
use stride::gateway::{ExecuteResult, GatewayCommand, GatewayMessage, GatewayServer};
use stride::ui::{self, Dialog, EditorState};
use std::sync::mpsc::{channel, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut config = EditorConfig::load(&EditorConfig::default_path()).unwrap_or_default();

    // Parse --port argument
    if let Some(pos) = args.iter().position(|a| a == "--port") {
        if let Some(port_str) = args.get(pos + 1) {
            if let Ok(port) = port_str.parse::<u16>() {
                config.gateway_port = port;
            }
        }
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

    // Check if port is available before starting TUI
    let port = state.config.gateway_port;
    if let Err(e) = std::net::TcpListener::bind(format!("127.0.0.1:{port}")) {
        eprintln!("Cannot start stride: port {port} is already in use ({e}).");
        eprintln!("Another stride instance may be running. Use )OFF or Ctrl+X to quit it first.");
        eprintln!("Or specify a different port with --port <number>.");
        std::process::exit(1);
    }

    // Start the gateway server (listens for interpreters to connect).
    let (server, gateway_rx, _gateway_tx) = GatewayServer::new(port);
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
            Ok(GatewayMessage::Connected { addr, info }) => {
                state.gateway_status = format!("connected to {addr} ({})", info.vendor);
            }
            Ok(GatewayMessage::Disconnected) => {
                state.gateway_status = format!("listening on port {}", state.config.gateway_port);
            }
            Ok(GatewayMessage::SessionOutput { text, output_type }) => {
                // Only display output types that are actual results (1, 2, 5, 7, 8, 11, 14)
                // Skip reserved types (0, 6, 10, 13) and status (9)
                match output_type {
                    0 | 6 | 10 | 13 => {} // reserved
                    9 => {} // status window info
                    _ => state.push_result(text),
                }
            }
            Ok(GatewayMessage::SetPromptType { prompt_type }) => {
                state.status = format!("prompt type: {}", prompt_type);
            }
            Ok(GatewayMessage::HadError) => {
                state.status = "error occurred".to_string();
            }
            Ok(GatewayMessage::GetLogReply { lines }) => {
                for line in lines {
                    state.push_result(line.text);
                }
            }
            Ok(GatewayMessage::InterpreterStatus { io, si, .. }) => {
                state.io_label = format!("⎕IO={}", io);
                state.status = format!("SI={}", si);
            }
            Ok(GatewayMessage::Configuration { name, value }) => {
                state.status = format!("config {} = {}", name, value);
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

/// Returns true when the app should quit.
fn handle_key(
    state: &mut EditorState,
    interpreter: &Arc<Mutex<Option<Sender<GatewayCommand>>>>,
    code: KeyCode,
    mods: KeyModifiers,
) -> bool {
    match (code, mods) {
        (KeyCode::Char('x'), KeyModifiers::CONTROL) => return true,
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
            eval_current_line(state, interpreter, 0); // execute (not trace)
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
        (KeyCode::Enter, KeyModifiers::CONTROL) => {
            // Execute all lines of current buffer
            let lines: Vec<String> = state.buffer().lines().iter().cloned().collect();
            for line in &lines {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    eval_line(state, interpreter, trimmed, 0);
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

fn eval_current_line(state: &mut EditorState, interpreter: &Arc<Mutex<Option<Sender<GatewayCommand>>>>, trace: u8) {
    let line = state.buffer_mut().current_line().trim().to_string();
    if line.is_empty() {
        return;
    }
    eval_line(state, interpreter, &line, trace);
}

fn eval_line(state: &mut EditorState, interpreter: &Arc<Mutex<Option<Sender<GatewayCommand>>>>, line: &str, trace: u8) {
    let (tx, rx) = channel::<ExecuteResult>();
    
    // Try to send to connected interpreter
    let sent = {
        let ints = interpreter.lock().unwrap();
        if let Some(ref sender) = *ints {
            sender.send(GatewayCommand::Execute {
                text: line.to_string(),
                trace,
                response_tx: tx,
            }).is_ok()
        } else {
            false
        }
    };
    
    if sent {
        // Wait for response (with timeout)
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => match result {
                ExecuteResult::Ok => {
                    state.status = "evaluated via gateway".to_string();
                }
                ExecuteResult::Error(e) => {
                    state.push_result(format!("ERROR: {e}"));
                    state.status = "eval error".to_string();
                }
            },
            Err(_) => {
                state.push_result("ERROR: gateway timeout".to_string());
                state.status = "gateway timeout".to_string();
            }
        }
    } else {
        state.push_result("ERROR: no interpreter connected".to_string());
        state.status = "no interpreter — start Kap or rust-apl with --ride".to_string();
    }
}

/// Handle keys when a dialog is open.
fn handle_dialog_key(state: &mut EditorState, code: KeyCode, mods: KeyModifiers) {
    if let Some(dialog) = &mut state.dialog {
        match dialog {
            Dialog::Help { scroll } => match code {
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
            },
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
