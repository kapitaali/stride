//! Gateway: TCP client to the interpreter's gateway port.
//!
//! Speaks the RIDE binary-framed protocol (same as the RIDE editor, src/cn.js):
//!   Framing: [4 bytes BE total length][4 bytes "RIDE"][JSON payload]
//!   Commands are JSON arrays: ["Name", {...}]
//!   Responses are JSON arrays: ["ReplyName", {...}]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Timeout for the initial gateway connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Client end of the interpreter gateway connection.
pub struct GatewayClient {
    stream: TcpStream,
    addr: String,
}

impl GatewayClient {
    /// Connect to the gateway at `host:port`.
    pub fn connect(host: &str, port: u16) -> std::io::Result<Self> {
        let addr = format!("{host}:{port}");
        let sock: std::net::SocketAddr = addr.parse().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("bad gateway addr: {e}"),
            )
        })?;
        let stream = TcpStream::connect_timeout(&sock, CONNECT_TIMEOUT)?;
        stream.set_nodelay(true).ok();
        Ok(Self { stream, addr })
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Perform the RIDE handshake. Must be called after connect.
    pub fn handshake(&mut self) -> std::io::Result<()> {
        // Step 1: send SupportedProtocols=2
        self.send_raw("SupportedProtocols=2")?;
        // Step 2: receive UsingProtocol=2
        let resp = self.recv_frame()?;
        if resp != "UsingProtocol=2" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unexpected handshake response: {resp}"),
            ));
        }
        // Step 3: send Identify
        self.send_frame(&serde_json::json!(["Identify", {"apiVersion":1,"identity":1}]).to_string())?;
        // Step 4: receive ReplyIdentify
        let _ = self.recv_frame()?;
        // Step 5: send Connect
        self.send_frame(&serde_json::json!(["Connect", {"remoteId":2}]).to_string())?;
        // Step 6: receive ReplyConnect
        let _ = self.recv_frame()?;
        Ok(())
    }

    /// Send a raw string frame (no JSON array wrapper).
    fn send_raw(&mut self, payload: &str) -> std::io::Result<()> {
        let total_len = (8 + payload.len()) as u32;
        self.stream.write_all(&total_len.to_be_bytes())?;
        self.stream.write_all(b"RIDE")?;
        self.stream.write_all(payload.as_bytes())?;
        self.stream.flush()?;
        Ok(())
    }

    /// Send a JSON payload frame.
    fn send_frame(&mut self, payload: &str) -> std::io::Result<()> {
        self.send_raw(payload)
    }

    /// Receive a single frame, return its payload.
    fn recv_frame(&mut self) -> std::io::Result<String> {
        let mut header = [0u8; 8];
        self.stream.read_exact(&mut header)?;
        let frame_len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        if frame_len < 8 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "frame too short",
            ));
        }
        let payload_len = frame_len - 8;
        let mut payload = vec![0u8; payload_len];
        self.stream.read_exact(&mut payload)?;
        String::from_utf8(payload).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid UTF-8 in frame")
        })
    }

    /// Evaluate an APL expression. Returns the result text on success,
    /// or the server's error message as `Err`.
    pub fn eval(&mut self, expr: &str) -> Result<String, String> {
        let cmd = serde_json::json!(["Execute", {"trace":0, "text": expr}]);
        self.send_frame(&cmd.to_string())
            .map_err(|e| format!("gateway I/O: {e}"))?;

        // Read frames until we get an AppendSessionOutput.
        loop {
            let payload = self.recv_frame().map_err(|e| format!("gateway I/O: {e}"))?;
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Some(arr) = val.as_array() {
                    if let Some(cmd) = arr[0].as_str() {
                        if cmd == "AppendSessionOutput" {
                            let result = arr[1]["result"].as_str().unwrap_or("");
                            if result.starts_with("ERROR ") {
                                return Err(result[6..].to_string());
                            }
                            return Ok(result.to_string());
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_refused_is_error() {
        // Port 1 is (practically) never listening; connect must fail fast.
        assert!(GatewayClient::connect("127.0.0.1", 1).is_err());
    }
}
