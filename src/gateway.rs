//! Gateway: TCP client to the interpreter's gateway port.
//!
//! Speaks the same line-based protocol as the rust-apl IPC shared-variable
//! server (`OFFER`/`QUERY`/`READ`/`WRITE`/`LIST`/`CANCEL`, see
//! `apl::ipc::protocol`), reusing its command/response types so the two
//! sides cannot drift apart. On top of that it offers `eval`, a RIDE-style
//! `EVAL <expr>` request: servers that implement evaluation answer with the
//! result line, while the current shared-variable server honestly replies
//! `ERROR unknown command` (surfaced to the caller, never hidden).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use apl::ipc::protocol::{IpcCommand, IpcResponse};

/// Timeout for the initial gateway connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Client end of the interpreter gateway connection.
pub struct GatewayClient {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
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
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self {
            stream,
            reader,
            addr,
        })
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    fn roundtrip(&mut self, line: &str) -> std::io::Result<String> {
        self.stream.write_all(line.as_bytes())?;
        self.stream.write_all(b"\n")?;
        self.stream.flush()?;
        let mut resp = String::new();
        self.reader.read_line(&mut resp)?;
        if resp.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "gateway closed the connection",
            ));
        }
        Ok(resp.trim_end_matches(['\r', '\n']).to_string())
    }

    /// Send one shared-variable command; parse the reply like `IpcClient` does.
    pub fn send(&mut self, cmd: &IpcCommand) -> std::io::Result<IpcResponse> {
        let line = self.roundtrip(&cmd.to_string())?;
        Ok(parse_response(&line))
    }

    /// RIDE-style evaluation request. Returns the raw result line on success,
    /// or the server's `ERROR ...` text as `Err`.
    pub fn eval(&mut self, expr: &str) -> Result<String, String> {
        let line = self
            .roundtrip(&format!("EVAL {expr}"))
            .map_err(|e| format!("gateway I/O: {e}"))?;
        match parse_response(&line) {
            IpcResponse::Error(msg) => Err(msg),
            other => Ok(other.to_string()),
        }
    }

    pub fn offer(&mut self, name: &str, value: &str) -> std::io::Result<IpcResponse> {
        self.send(&IpcCommand::Offer {
            name: name.to_string(),
            value: value.to_string(),
        })
    }

    pub fn query(&mut self, name: &str) -> std::io::Result<IpcResponse> {
        self.send(&IpcCommand::Query {
            name: name.to_string(),
        })
    }

    pub fn read(&mut self, name: &str) -> std::io::Result<IpcResponse> {
        self.send(&IpcCommand::Read {
            name: name.to_string(),
        })
    }

    pub fn write(&mut self, name: &str, value: &str) -> std::io::Result<IpcResponse> {
        self.send(&IpcCommand::Write {
            name: name.to_string(),
            value: value.to_string(),
        })
    }

    pub fn list(&mut self) -> std::io::Result<IpcResponse> {
        self.send(&IpcCommand::List)
    }

    pub fn cancel(&mut self, name: &str) -> std::io::Result<IpcResponse> {
        self.send(&IpcCommand::Cancel {
            name: name.to_string(),
        })
    }
}

/// Parse a server reply line with the same rules as `apl::ipc::client`.
fn parse_response(line: &str) -> IpcResponse {
    if line == "OK" {
        IpcResponse::Ok
    } else if let Some(msg) = line.strip_prefix("ERROR ") {
        IpcResponse::Error(msg.to_string())
    } else if let Ok(n) = line.parse::<i64>() {
        IpcResponse::Int(n)
    } else {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() > 1 {
            IpcResponse::Names(parts.iter().map(|s| s.to_string()).collect())
        } else {
            IpcResponse::Value(line.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ok_error_int_value() {
        assert_eq!(parse_response("OK"), IpcResponse::Ok);
        assert_eq!(
            parse_response("ERROR unknown command: EVAL"),
            IpcResponse::Error("unknown command: EVAL".to_string())
        );
        assert_eq!(parse_response("1"), IpcResponse::Int(1));
        assert_eq!(parse_response("42"), IpcResponse::Int(42),);
        assert_eq!(
            parse_response("hello"),
            IpcResponse::Value("hello".to_string())
        );
    }

    #[test]
    fn connect_refused_is_error() {
        // Port 1 is (practically) never listening; connect must fail fast.
        assert!(GatewayClient::connect("127.0.0.1", 1).is_err());
    }
}
