//! Gateway: RIDE protocol server.
//!
//! Conforms to the RIDE protocol specification from ride/docs/protocol.md.
//!
//! Wire format (after handshake):
//!   [4 bytes BE total length][4 bytes "RIDE"][UTF-8 JSON payload]
//!   Total length = 8 + len(payload in bytes)
//!
//! Handshake:
//!   The client opens with "SupportedProtocols=2"; the server answers with
//!   "UsingProtocol=2". Real interpreters frame these like any other message;
//!   the reply mirrors the client's framing, so both strictly-framed clients
//!   (Kap, Dyalog) and plain-text clients (rust-apl) are served:
//!     - framed client: [SupportedProtocols=2][UsingProtocol=2] as frames
//!     - raw client:    UsingProtocol=2 as plain text
//!
//!   After the protocol is agreed the interpreter sends ["Identify",{...}]
//!   and the server replies ["ReplyIdentify",{...}], then the interpreter
//!   sends ["Connect",{"remoteId":N}] and the server replies
//!   ["ReplyConnect",{...}]. From then on messages are independent JSON
//!   2-element arrays: ["Name", {...}].

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

fn debug_log(msg: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/stride_debug.log")
    {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Messages from the gateway server to the UI.
#[derive(Debug, Clone)]
pub enum GatewayMessage {
    Connected {
        addr: String,
        info: InterpreterInfo,
    },
    Disconnected,
    SessionOutput {
        text: String,
        output_type: u8,
    },
    SetPromptType {
        prompt_type: u8,
    },
    HadError,
    GetLogReply {
        lines: Vec<LogLine>,
    },
    InterpreterStatus {
        io: i64,
        dq: i64,
        wa: i64,
        si: i64,
        trap: i64,
        ml: i64,
        num_threads: i64,
        tid: i64,
    },
    Configuration {
        name: String,
        value: String,
    },
}

/// Log line with type and group.
#[derive(Debug, Clone)]
pub struct LogLine {
    pub text: String,
    pub output_type: u8,
    pub group: u8,
}

/// Interpreter information from ReplyIdentify.
#[derive(Debug, Clone)]
pub struct InterpreterInfo {
    pub vendor: String,
    pub language: String,
    pub version: String,
    pub platform: String,
    pub project: String,
    pub process: String,
    pub user: String,
    pub pid: i64,
    pub arch: String,
}

impl Default for InterpreterInfo {
    fn default() -> Self {
        Self {
            vendor: String::new(),
            language: "APL".to_string(),
            version: String::new(),
            platform: String::new(),
            project: String::new(),
            process: String::new(),
            user: String::new(),
            pid: 0,
            arch: String::new(),
        }
    }
}

/// Commands from the UI to the gateway server.
#[derive(Debug)]
pub enum GatewayCommand {
    Execute {
        text: String,
        trace: u8,
        response_tx: Sender<ExecuteResult>,
    },
    GetLog {
        max_lines: i64,
        response_tx: Sender<Vec<LogLine>>,
    },
    SetPW {
        pw: u16,
    },
    Exit,
}

/// Result of an Execute command.
#[derive(Debug)]
pub enum ExecuteResult {
    Ok,
    Error(String),
}

/// Gateway server that listens for interpreters to connect.
pub struct GatewayServer {
    pub port: u16,
    pub host: String,
    tx: Sender<GatewayMessage>,
    interpreter: Arc<Mutex<Option<Sender<GatewayCommand>>>>,
}

impl GatewayServer {
    pub fn new(
        host: String,
        port: u16,
    ) -> (Self, Receiver<GatewayMessage>, Sender<GatewayCommand>) {
        let (tx, ui_rx) = channel::<GatewayMessage>();
        let (ui_tx, _rx) = channel::<GatewayCommand>();
        let interpreter = Arc::new(Mutex::new(None));

        (
            GatewayServer {
                host,
                port,
                tx,
                interpreter,
            },
            ui_rx,
            ui_tx,
        )
    }

    pub fn interpreter(&self) -> Arc<Mutex<Option<Sender<GatewayCommand>>>> {
        self.interpreter.clone()
    }

    pub fn run(self, listener: TcpListener) -> std::io::Result<()> {
        let addr = listener
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| format!("{}:{}", self.host, self.port));

        let tx = self.tx.clone();
        let interpreter = self.interpreter.clone();
        let current_conn = Arc::new(AtomicU64::new(0));
        let mut next_conn: u64 = 1;

        debug_log(&format!("[gateway] waiting for connections on {}...", addr));
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let peer = stream
                        .peer_addr()
                        .map(|a| a.to_string())
                        .unwrap_or_default();
                    debug_log(&format!("[gateway] interpreter connected from {}", peer));
                    let tx = tx.clone();
                    let interpreter = interpreter.clone();
                    let current_conn = current_conn.clone();
                    let conn_id = next_conn;
                    next_conn += 1;

                    thread::spawn(move || {
                        handle_interpreter(stream, tx, interpreter, peer, conn_id, current_conn);
                    });
                }
                Err(e) => {
                    debug_log(&format!("[gateway] accept error: {}", e));
                }
            }
        }

        Ok(())
    }
}

fn handle_interpreter(
    mut stream: TcpStream,
    tx: Sender<GatewayMessage>,
    interpreter: Arc<Mutex<Option<Sender<GatewayCommand>>>>,
    addr: String,
    conn_id: u64,
    current_conn: Arc<AtomicU64>,
) {
    debug_log(&format!(
        "[gateway] interpreter connected from {} (conn {})",
        addr, conn_id
    ));

    // Protocol handshake: read the client's SupportedProtocols=2 and answer.
    if let Err(e) = perform_handshake(&mut stream) {
        debug_log(&format!("[gateway] handshake failed: {}", e));
        let _ = tx.send(GatewayMessage::SessionOutput {
            text: format!("gateway: handshake failed: {}", e),
            output_type: 3,
        });
        return;
    }

    // The connection is served by two threads: this one reads messages from
    // the interpreter, a second one writes commands sent by the UI. Both use
    // the same write half, serialized by the mutex so frames never interleave.
    let wstream = match stream.try_clone() {
        Ok(s) => Arc::new(Mutex::new(s)),
        Err(e) => {
            debug_log(&format!("[gateway] stream clone failed: {}", e));
            return;
        }
    };

    let (cmd_tx, cmd_rx) = channel::<GatewayCommand>();

    current_conn.store(conn_id, Ordering::SeqCst);
    {
        let mut ints = interpreter.lock().unwrap();
        *ints = Some(cmd_tx.clone());
    }

    // Command writer thread: UI -> interpreter.
    {
        let wstream = wstream.clone();
        let tx = tx.clone();
        thread::spawn(move || {
            for cmd in cmd_rx {
                match cmd {
                    GatewayCommand::Execute {
                        text,
                        trace,
                        response_tx,
                    } => {
                        let msg = serde_json::json!([
                            "Execute",
                            { "text": text, "trace": trace }
                        ]);
                        match write_frame_locked(&wstream, &msg.to_string()) {
                            Ok(()) => {
                                let _ = response_tx.send(ExecuteResult::Ok);
                            }
                            Err(e) => {
                                let _ = response_tx.send(ExecuteResult::Error(e.clone()));
                                let _ = tx.send(GatewayMessage::SessionOutput {
                                    text: e,
                                    output_type: 3,
                                });
                            }
                        }
                    }
                    GatewayCommand::GetLog {
                        max_lines,
                        response_tx,
                    } => {
                        let msg = serde_json::json!([
                            "GetLog",
                            { "format": "json", "maxLines": max_lines }
                        ]);
                        if write_frame_locked(&wstream, &msg.to_string()).is_ok() {
                            let _ = response_tx.send(Vec::new());
                        }
                    }
                    GatewayCommand::SetPW { pw } => {
                        let msg = serde_json::json!(["SetPW", { "pw": pw }]);
                        let _ = write_frame_locked(&wstream, &msg.to_string());
                    }
                    GatewayCommand::Exit => {
                        let msg = serde_json::json!(["Exit", { "code": 0 }]);
                        let _ = write_frame_locked(&wstream, &msg.to_string());
                        break;
                    }
                }
            }
            debug_log(&format!(
                "[gateway] command writer for conn {} exiting",
                conn_id
            ));
        });
    }

    // The protocol is agreed: the UI may now talk to this interpreter.
    let _ = tx.send(GatewayMessage::Connected {
        addr: addr.clone(),
        info: InterpreterInfo::default(),
    });

    // Reader loop: interpreter -> UI.
    loop {
        let payload = match read_frame(&mut stream) {
            Ok(p) => p,
            Err(e) => {
                debug_log(&format!("[gateway] connection {} ended: {}", conn_id, e));
                break;
            }
        };
        debug_log(&format!("[gateway] recv: {}", payload));

        let value: serde_json::Value = match serde_json::from_str(&payload) {
            Ok(v) => v,
            Err(e) => {
                debug_log(&format!("[gateway] non-JSON message ignored: {}", e));
                continue;
            }
        };
        let arr = match value.as_array() {
            Some(a) => a,
            None => continue,
        };
        let cmd = arr.first().and_then(|c| c.as_str()).unwrap_or("");
        let args = arr.get(1).cloned().unwrap_or(serde_json::Value::Null);

        match cmd {
            "Identify" => {
                // The interpreter announces itself and waits for ReplyIdentify.
                let reply = serde_json::json!([
                    "ReplyIdentify",
                    {
                        "apiVersion": 1,
                        "identity": 2,
                        "Port": 0,
                        "IPAddress": "",
                        "Vendor": "",
                        "Language": "APL",
                        "version": "",
                        "Machine": machine_name(),
                        "arch": "64",
                        "Project": "CLEAR WS",
                        "Process": "apl",
                        "User": user_name(),
                        "pid": 0,
                        "token": "",
                        "date": "",
                        "platform": platform_name()
                    }
                ]);
                debug_log(&format!("[gateway] replying to Identify: {}", reply));
                if write_frame_locked(&wstream, &reply.to_string()).is_err() {
                    break;
                }
            }
            "Connect" => {
                let reply = serde_json::json!([
                    "ReplyConnect",
                    { "remoteId": args["remoteId"].clone(), "protocolVersion": 2 }
                ]);
                debug_log(&format!("[gateway] replying to Connect: {}", reply));
                if write_frame_locked(&wstream, &reply.to_string()).is_err() {
                    break;
                }
            }
            "AppendSessionOutput" => {
                let text = args["result"].as_str().unwrap_or("").to_string();
                let output_type = args["type"].as_u64().unwrap_or(1) as u8;
                let _ = tx.send(GatewayMessage::SessionOutput { text, output_type });
            }
            "EchoInput" => {
                let text = args["input"].as_str().unwrap_or("").to_string();
                let _ = tx.send(GatewayMessage::SessionOutput {
                    text,
                    output_type: 14,
                });
            }
            "ReplyGetLog" => {
                if let Ok(lines) = parse_get_log_response(&value) {
                    let _ = tx.send(GatewayMessage::GetLogReply { lines });
                }
            }
            other => {
                debug_log(&format!(
                    "[gateway] unhandled message from interpreter: {}",
                    other
                ));
            }
        }
    }

    // Cleanup: only clear the interpreter slot if this connection still owns it.
    {
        let mut ints = interpreter.lock().unwrap();
        if current_conn.load(Ordering::SeqCst) == conn_id {
            *ints = None;
        }
    }
    let _ = tx.send(GatewayMessage::Disconnected);
    debug_log(&format!("[gateway] connection {} closed", conn_id));
}

/// Read the client's opening handshake bytes and detect its framing style.
/// Returns `(bytes, framed)`: `framed == true` when the payload starts with a
/// RIDE frame header (e.g. Kap), `false` for plain text clients (rust-apl).
/// Waits for the complete first frame: strictly-framed clients flush each
/// handshake block separately, so the header alone proves nothing yet.
fn read_client_hello(stream: &mut TcpStream) -> Result<(Vec<u8>, bool), String> {
    let mut buf = [0u8; 4096];
    let mut collected: Vec<u8> = Vec::new();

    for _ in 0..8 {
        let n = stream
            .read(&mut buf)
            .map_err(|e| format!("read error: {}", e))?;
        if n == 0 {
            return Err("connection closed during handshake".to_string());
        }
        collected.extend_from_slice(&buf[..n]);

        // A framed client starts with [len]["RIDE"]; only decide once the
        // whole first frame is here, otherwise a bare 8-byte header (Kap
        // flushes per block) would pass with no payload to check.
        if collected.len() >= 8 && &collected[4..8] == b"RIDE" {
            let frame_len =
                u32::from_be_bytes([collected[0], collected[1], collected[2], collected[3]])
                    as usize;
            if frame_len >= 8 && collected.len() >= frame_len {
                debug_log(&format!(
                    "[gateway] client hello: {} bytes, framed",
                    collected.len()
                ));
                return Ok((collected, true));
            }
            // Header complete but payload still in flight: keep reading.
            continue;
        }
        if collected
            .windows(b"SupportedProtocols=2".len())
            .any(|w| w == b"SupportedProtocols=2")
        {
            debug_log(&format!(
                "[gateway] client hello: {} bytes, raw",
                collected.len()
            ));
            return Ok((collected, false));
        }
    }

    Err(format!(
        "unexpected handshake from client: {:?}",
        String::from_utf8_lossy(&collected)
    ))
}

/// The RIDE protocol handshake: the client sends "SupportedProtocols=2" and
/// the server answers "UsingProtocol=2". The reply mirrors the client's
/// framing so both framed (Kap) and plain-text (rust-apl) clients work.
fn perform_handshake(stream: &mut TcpStream) -> Result<(), String> {
    debug_log("[gateway] starting handshake...");
    let (hello, framed) = read_client_hello(stream)?;

    let has_supported = hello
        .windows(b"SupportedProtocols=2".len())
        .any(|w| w == b"SupportedProtocols=2");
    if !has_supported {
        return Err(format!(
            "client did not send SupportedProtocols=2 (got {:?})",
            String::from_utf8_lossy(&hello)
        ));
    }
    debug_log("[gateway] client sent SupportedProtocols=2");

    if framed {
        // Strictly framed clients expect framed blocks back, in order.
        let mut blob = frame("SupportedProtocols=2");
        blob.extend_from_slice(&frame("UsingProtocol=2"));
        stream
            .write_all(&blob)
            .map_err(|e| format!("write error: {}", e))?;
        stream.flush().map_err(|e| format!("flush error: {}", e))?;
        debug_log("[gateway] sent framed SupportedProtocols=2 + UsingProtocol=2");
    } else {
        // Plain-text clients scan the reply for UsingProtocol=2.
        stream
            .write_all(b"UsingProtocol=2")
            .map_err(|e| format!("write error: {}", e))?;
        stream.flush().map_err(|e| format!("flush error: {}", e))?;
        debug_log("[gateway] sent raw UsingProtocol=2");
    }

    debug_log("[gateway] protocol handshake complete");
    Ok(())
}

fn write_frame_locked(stream: &Arc<Mutex<TcpStream>>, payload: &str) -> Result<(), String> {
    let f = frame(payload);
    let mut s = stream.lock().unwrap();
    s.write_all(&f).map_err(|e| format!("write error: {}", e))?;
    s.flush().map_err(|e| format!("flush error: {}", e))?;
    Ok(())
}

fn machine_name() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()))
        .unwrap_or_default()
}

fn user_name() -> String {
    std::env::var("USER").unwrap_or_default()
}

fn platform_name() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn parse_get_log_response(value: &serde_json::Value) -> Result<Vec<LogLine>, String> {
    let arr = value.as_array().ok_or("ReplyGetLog is not an array")?;
    let result = arr.get(1).and_then(|o| o["result"].as_array());

    let mut lines = Vec::new();
    if let Some(items) = result {
        for item in items {
            if let Some(text) = item.as_str() {
                lines.push(LogLine {
                    text: text.to_string(),
                    output_type: 1,
                    group: 1,
                });
            } else if let Some(obj) = item.as_object() {
                lines.push(LogLine {
                    text: obj["text"].as_str().unwrap_or("").to_string(),
                    output_type: obj["type"].as_u64().unwrap_or(1) as u8,
                    group: obj["group"].as_u64().unwrap_or(1) as u8,
                });
            }
        }
    }
    Ok(lines)
}

fn read_frame(stream: &mut TcpStream) -> Result<String, String> {
    let mut header = [0u8; 8];
    stream
        .read_exact(&mut header)
        .map_err(|e| format!("read error: {}", e))?;

    let frame_len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    if frame_len < 8 {
        return Err("frame too short".to_string());
    }

    if &header[4..8] != b"RIDE" {
        return Err("invalid frame magic".to_string());
    }

    let payload_len = frame_len - 8;
    let mut payload = vec![0u8; payload_len];
    stream
        .read_exact(&mut payload)
        .map_err(|e| format!("read error: {}", e))?;

    String::from_utf8(payload).map_err(|_| "invalid UTF-8".to_string())
}

fn frame(payload: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + payload.len());
    let total_len = (8 + payload.len()) as u32;
    buf.extend_from_slice(&total_len.to_be_bytes());
    buf.extend_from_slice(b"RIDE");
    buf.extend_from_slice(payload.as_bytes());
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame() {
        let f = frame("test");
        assert_eq!(f.len(), 12);
        assert_eq!(&f[4..8], b"RIDE");
        assert_eq!(&f[8..], b"test");
    }

    #[test]
    fn test_frame_json() {
        let payload = r#"["Execute",{"text":"2+2","trace":0}]"#;
        let f = frame(payload);
        assert_eq!(f.len(), 8 + payload.len());
        assert_eq!(&f[4..8], b"RIDE");
    }

    #[test]
    fn test_parse_get_log_text() {
        let value: serde_json::Value = serde_json::json!([
            "ReplyGetLog",
            { "result": ["line 1", "line 2"] }
        ]);
        let lines = parse_get_log_response(&value).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "line 1");
    }

    #[test]
    fn test_parse_get_log_json() {
        let value: serde_json::Value = serde_json::json!([
            "ReplyGetLog",
            {
                "result": [
                    { "group": 1, "type": 1, "text": "line 1" },
                    { "group": 1, "type": 5, "text": "error" }
                ]
            }
        ]);
        let lines = parse_get_log_response(&value).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].output_type, 1);
        assert_eq!(lines[1].output_type, 5);
    }
}
