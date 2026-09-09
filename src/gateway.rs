//! Gateway: TCP server that listens for interpreters to connect.
//!
//! Speaks the RIDE binary-framed protocol (same as the RIDE editor, src/cn.js):
//!   Framing: [4 bytes BE total length][4 bytes "RIDE"][JSON payload]
//!   Commands: JSON arrays like ["Execute", {"text": "2+2"}]
//!   Responses: JSON arrays like ["AppendSessionOutput", {"result": "4"}]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// Messages from the gateway server to the UI.
#[derive(Debug, Clone)]
pub enum GatewayMessage {
    /// An interpreter has connected.
    Connected { addr: String },
    /// The interpreter has disconnected.
    Disconnected,
    /// Output from an expression evaluation.
    Output { result: String, msg_type: u8 },
}

/// Commands from the UI to the gateway server.
#[derive(Debug)]
pub enum GatewayCommand {
    /// Evaluate an expression, return the result via the channel.
    Execute {
        text: String,
        response_tx: Sender<String>,
    },
}

/// Gateway server that listens for interpreters to connect.
pub struct GatewayServer {
    pub port: u16,
    /// Send messages to the UI.
    tx: Sender<GatewayMessage>,
    /// Current interpreter connection (if any).
    interpreter: Arc<Mutex<Option<Sender<GatewayCommand>>>>,
}

impl GatewayServer {
    /// Create a new gateway server on the given port.
    pub fn new(port: u16) -> (Self, Receiver<GatewayMessage>, Sender<GatewayCommand>) {
        let (tx, ui_rx) = channel::<GatewayMessage>();
        let (ui_tx, _rx) = channel::<GatewayCommand>();
        let interpreter = Arc::new(Mutex::new(None));

        (
            GatewayServer { port, tx, interpreter },
            ui_rx,
            ui_tx,
        )
    }

    /// Get a reference to the interpreter sender (for the UI to send commands).
    pub fn interpreter(&self) -> Arc<Mutex<Option<Sender<GatewayCommand>>>> {
        self.interpreter.clone()
    }

    /// Start the gateway server. This method blocks.
    pub fn run(self) -> std::io::Result<()> {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", self.port)) {
            Ok(l) => l,
            Err(e) => {
                let _ = self.tx.send(GatewayMessage::Output {
                    result: format!("gateway: cannot bind port {}: {}", self.port, e),
                    msg_type: 0,
                });
                return Err(e);
            }
        };

        let tx = self.tx.clone();
        let interpreter = self.interpreter.clone();

        // Accept connections.
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let addr = stream
                        .peer_addr()
                        .map(|a| a.to_string())
                        .unwrap_or_default();
                    let tx = tx.clone();
                    let interpreter = interpreter.clone();

                    // Handle this connection in a new thread.
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

/// Handle a single interpreter connection.
fn handle_interpreter(
    mut stream: TcpStream,
    tx: Sender<GatewayMessage>,
    interpreter: Arc<Mutex<Option<Sender<GatewayCommand>>>>,
    addr: String,
) {
    // Perform the handshake.
    if !perform_handshake(&mut stream) {
        return;
    }

    // Create a channel for sending commands to this interpreter.
    let (cmd_tx, cmd_rx) = channel::<GatewayCommand>();

    // Register this interpreter.
    {
        let mut ints = interpreter.lock().unwrap();
        *ints = Some(cmd_tx.clone());
    }

    // Notify UI that interpreter is connected.
    let _ = tx.send(GatewayMessage::Connected { addr: addr.clone() });

    // Process commands from the UI.
    for cmd in cmd_rx {
        match cmd {
            GatewayCommand::Execute { text, response_tx } => {
                let result = execute_expression(&mut stream, &text);
                let _ = response_tx.send(result.clone());
                let _ = tx.send(GatewayMessage::Output {
                    result,
                    msg_type: 0,
                });
            }
        }
    }

    // Unregister this interpreter.
    {
        let mut ints = interpreter.lock().unwrap();
        *ints = None;
    }

    // Channel closed, interpreter disconnected.
    let _ = tx.send(GatewayMessage::Disconnected);
}

/// Perform the RIDE handshake with the interpreter.
fn perform_handshake(stream: &mut TcpStream) -> bool {
    // Step 1: Read SupportedProtocols=2 from interpreter
    let mut buf = [0u8; 4096];
    let n = match stream.read(&mut buf) {
        Ok(0) => return false,
        Ok(n) => n,
        Err(_) => return false,
    };

    let payload = String::from_utf8_lossy(&buf[..n]);
    if !payload.contains("SupportedProtocols=2") {
        return false;
    }

    // Step 2: Send UsingProtocol=2
    let response = "UsingProtocol=2";
    let frame = frame(response);
    if stream.write_all(&frame).is_err() {
        return false;
    }

    // Step 3: Read ["Identify", {...}] from interpreter
    let identify = match read_frame(stream) {
        Ok(msg) => msg,
        Err(_) => return false,
    };

    if !identify.contains("Identify") {
        return false;
    }

    // Step 4: Send ["ReplyIdentify", {...}]
    let reply = serde_json::json!(["ReplyIdentify", {
        "identity": 1,
        "protocolVersion": 2,
        "version": "stride 0.1.0"
    }]);
    if write_frame(stream, &reply.to_string()).is_err() {
        return false;
    }

    // Step 5: Read ["Connect", {...}] from interpreter
    let connect = match read_frame(stream) {
        Ok(msg) => msg,
        Err(_) => return false,
    };

    if !connect.contains("Connect") {
        return false;
    }

    // Step 6: Send ["ReplyConnect", {...}]
    let reply = serde_json::json!(["ReplyConnect", {
        "remoteId": 2,
        "protocolVersion": 2
    }]);
    if write_frame(stream, &reply.to_string()).is_err() {
        return false;
    }

    true
}

/// Execute an expression on the interpreter and return the result.
fn execute_expression(stream: &mut TcpStream, text: &str) -> String {
    let cmd = serde_json::json!(["Execute", {"trace":0, "text": text}]);
    if write_frame(stream, &cmd.to_string()).is_err() {
        return "ERROR: failed to send expression".to_string();
    }

    // Read response
    match read_frame(stream) {
        Ok(response) => response,
        Err(e) => format!("ERROR: {}", e),
    }
}

/// Read a framed message from the stream.
fn read_frame(stream: &mut TcpStream) -> Result<String, String> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header).map_err(|e| format!("read error: {}", e))?;

    let frame_len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    if frame_len < 8 {
        return Err("frame too short".to_string());
    }

    // Verify "RIDE" magic
    if &header[4..8] != b"RIDE" {
        return Err("invalid frame magic".to_string());
    }

    let payload_len = frame_len - 8;
    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).map_err(|e| format!("read error: {}", e))?;

    String::from_utf8(payload).map_err(|_| "invalid UTF-8".to_string())
}

/// Write a framed message to the stream.
fn write_frame(stream: &mut TcpStream, payload: &str) -> Result<(), String> {
    let frame = frame(payload);
    stream.write_all(&frame).map_err(|e| format!("write error: {}", e))?;
    stream.flush().map_err(|e| format!("flush error: {}", e))?;
    Ok(())
}

/// Frame a JSON payload for the RIDE protocol.
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
        assert_eq!(f.len(), 12); // 4 bytes len + 4 bytes "RIDE" + 4 bytes "test"
        assert_eq!(&f[4..8], b"RIDE");
        assert_eq!(&f[8..], b"test");
    }
}
