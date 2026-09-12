//! Gateway: RIDE protocol server.
//!
//! Conforms to the RIDE protocol specification from ride/docs/protocol.md.
//!
//! Wire format:
//!   [4 bytes BE total length][4 bytes "RIDE"][UTF-8 JSON payload]
//!   Total length = 8 + len(payload in bytes)
//!
//! Handshake (first two messages, NOT JSON-encoded):
//!   1. Both sides send "SupportedProtocols=2"
//!   2. Both sides send "UsingProtocol=2"
//!
//! After handshake, all messages are JSON 2-element arrays: ["Name", {...}]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

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
    tx: Sender<GatewayMessage>,
    interpreter: Arc<Mutex<Option<Sender<GatewayCommand>>>>,
}

impl GatewayServer {
    pub fn new(port: u16) -> (Self, Receiver<GatewayMessage>, Sender<GatewayCommand>) {
        let (tx, ui_rx) = channel::<GatewayMessage>();
        let (ui_tx, _rx) = channel::<GatewayCommand>();
        let interpreter = Arc::new(Mutex::new(None));

        (
            GatewayServer {
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

    pub fn run(self) -> std::io::Result<()> {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", self.port)) {
            Ok(l) => l,
            Err(e) => {
                let _ = self.tx.send(GatewayMessage::SessionOutput {
                    text: format!("gateway: cannot bind port {}: {}", self.port, e),
                    output_type: 3,
                });
                return Err(e);
            }
        };

        let tx = self.tx.clone();
        let interpreter = self.interpreter.clone();

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let addr = stream
                        .peer_addr()
                        .map(|a| a.to_string())
                        .unwrap_or_default();
                    let tx = tx.clone();
                    let interpreter = interpreter.clone();

                    thread::spawn(move || {
                        handle_interpreter(stream, tx, interpreter, addr);
                    });
                }
                Err(_) => {}
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
) {
    let info = match perform_handshake(&mut stream) {
        Some(info) => info,
        None => return,
    };

    let (cmd_tx, cmd_rx) = channel::<GatewayCommand>();

    {
        let mut ints = interpreter.lock().unwrap();
        *ints = Some(cmd_tx.clone());
    }

    let _ = tx.send(GatewayMessage::Connected {
        addr: addr.clone(),
        info,
    });

    for cmd in cmd_rx {
        match cmd {
            GatewayCommand::Execute {
                text,
                trace,
                response_tx,
            } => match send_execute(&mut stream, &text, trace) {
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
            },
            GatewayCommand::GetLog {
                max_lines,
                response_tx,
            } => match send_get_log(&mut stream, max_lines) {
                Ok(lines) => {
                    let _ = tx.send(GatewayMessage::GetLogReply {
                        lines: lines.clone(),
                    });
                    let _ = response_tx.send(lines);
                }
                Err(e) => {
                    let _ = tx.send(GatewayMessage::SessionOutput {
                        text: e,
                        output_type: 3,
                    });
                }
            },
            GatewayCommand::SetPW { pw } => {
                let _ = send_set_pw(&mut stream, pw);
            }
            GatewayCommand::Exit => {
                let _ = send_exit(&mut stream);
                break;
            }
        }
    }

    {
        let mut ints = interpreter.lock().unwrap();
        *ints = None;
    }

    let _ = tx.send(GatewayMessage::Disconnected);
}

fn read_handshake_response(stream: &mut TcpStream, expected: &str) -> bool {
    let mut buf = [0u8; 1024];
    match stream.read(&mut buf) {
        Ok(n) if n > 0 => {
            let response = String::from_utf8_lossy(&buf[..n]);
            response.contains(expected)
        }
        _ => false,
    }
}

fn send_handshake_message(stream: &mut TcpStream, msg: &str) -> bool {
    stream.write_all(msg.as_bytes()).is_ok() && stream.flush().is_ok()
}

fn perform_handshake(stream: &mut TcpStream) -> Option<InterpreterInfo> {
    if !send_handshake_message(stream, "SupportedProtocols=2") {
        return None;
    }

    if !read_handshake_response(stream, "SupportedProtocols=2") {
        return None;
    }

    if !send_handshake_message(stream, "UsingProtocol=2") {
        return None;
    }

    if !read_handshake_response(stream, "UsingProtocol=2") {
        return None;
    }

    let identify_raw = match read_frame(stream) {
        Ok(msg) => msg,
        Err(_) => return None,
    };

    let identify: serde_json::Value = match serde_json::from_str(&identify_raw) {
        Ok(v) => v,
        Err(_) => return None,
    };

    let api_version = identify
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(|obj| obj["apiVersion"].as_i64())
        .unwrap_or(0);

    let our_identify = serde_json::json!([
        "Identify",
        {
            "apiVersion": api_version,
            "identity": 1
        }
    ]);
    if write_frame(stream, &our_identify.to_string()).is_err() {
        return None;
    }

    let reply_raw = match read_frame(stream) {
        Ok(msg) => msg,
        Err(_) => return None,
    };

    let reply: serde_json::Value = match serde_json::from_str(&reply_raw) {
        Ok(v) => v,
        Err(_) => return None,
    };

    let info = if let Some(arr) = reply.as_array() {
        if let Some(obj) = arr.get(1) {
            InterpreterInfo {
                vendor: obj["Vendor"].as_str().unwrap_or("").to_string(),
                language: obj["Language"].as_str().unwrap_or("APL").to_string(),
                version: obj["version"].as_str().unwrap_or("").to_string(),
                platform: obj["platform"].as_str().unwrap_or("").to_string(),
                project: obj["Project"].as_str().unwrap_or("").to_string(),
                process: obj["Process"].as_str().unwrap_or("").to_string(),
                user: obj["User"].as_str().unwrap_or("").to_string(),
                pid: obj["pid"].as_i64().unwrap_or(0),
                arch: obj["arch"].as_str().unwrap_or("").to_string(),
            }
        } else {
            InterpreterInfo::default()
        }
    } else {
        InterpreterInfo::default()
    };

    Some(info)
}

fn send_execute(stream: &mut TcpStream, text: &str, trace: u8) -> Result<(), String> {
    let cmd = serde_json::json!([
        "Execute",
        {
            "text": text,
            "trace": trace
        }
    ]);
    write_frame(stream, &cmd.to_string())
        .map_err(|e| format!("failed to send Execute: {}", e))
}

fn send_get_log(stream: &mut TcpStream, max_lines: i64) -> Result<Vec<LogLine>, String> {
    let cmd = serde_json::json!([
        "GetLog",
        {
            "format": "json",
            "maxLines": max_lines
        }
    ]);
    write_frame(stream, &cmd.to_string())
        .map_err(|e| format!("failed to send GetLog: {}", e))?;

    let response = read_frame(stream).map_err(|e| format!("failed to read ReplyGetLog: {}", e))?;
    let value: serde_json::Value = serde_json::from_str(&response)
        .map_err(|e| format!("invalid JSON in ReplyGetLog: {}", e))?;

    parse_get_log_response(&value)
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

fn send_set_pw(stream: &mut TcpStream, pw: u16) -> Result<(), String> {
    let cmd = serde_json::json!(["SetPW", {"pw": pw}]);
    write_frame(stream, &cmd.to_string())
        .map_err(|e| format!("failed to send SetPW: {}", e))
}

fn send_exit(stream: &mut TcpStream) -> Result<(), String> {
    let cmd = serde_json::json!(["Exit", {"code": 0}]);
    write_frame(stream, &cmd.to_string())
        .map_err(|e| format!("failed to send Exit: {}", e))
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

fn write_frame(stream: &mut TcpStream, payload: &str) -> Result<(), String> {
    let f = frame(payload);
    stream
        .write_all(&f)
        .map_err(|e| format!("write error: {}", e))?;
    stream.flush().map_err(|e| format!("flush error: {}", e))?;
    Ok(())
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
