# RIDE Protocol Conformance Plan

## Goal

Make stride conform to the RIDE protocol: stride is the **server** (listens on a port), and language interpreters (Kap, etc.) are **clients** that connect to stride.

## Current Architecture (wrong)

```
stride (TCP client) ──connects to──→ apl --serve (TCP server)
  - stride spawns the interpreter
  - stride sends ["Execute", {"text": "..."}]
  - stride receives ["AppendSessionOutput", {"result": "..."}]
```

## Target Architecture (correct)

```
stride (TCP server, listens on 4502) ←── Kap --ride (TCP client)
  - stride listens for interpreters to connect
  - interpreter sends ["Identify", {...}]
  - stride replies ["ReplyIdentify", {...}]
  - user presses Ctrl+E in stride
  - stride sends ["Execute", {"text": "..."}] to interpreter
  - interpreter sends ["AppendSessionOutput", {"result": "..."}] back
```

## Protocol Reference (from RIDE src/cn.js)

### Handshake (interpreter → stride)

1. Interpreter sends: `SupportedProtocols=2`
2. Stride replies: `UsingProtocol=2`
3. Interpreter sends: `["Identify", {"apiVersion":1, "identity":1}]`
4. Stride replies: `["ReplyIdentify", {"identity":1, "protocolVersion":2, "version":"..."}]`
5. Interpreter sends: `["Connect", {"remoteId":2}]`
6. Stride replies: `["ReplyConnect", {"remoteId":2, "protocolVersion":2}]`

### Expression Evaluation

- Stride sends: `["Execute", {"trace":0, "text":"2+2\n"}]`
- Interpreter replies: `["AppendSessionOutput", {"result":"4", "group":0, "type":0}]`

### Framing

All messages use binary framing: `[4 bytes BE length][4 bytes "RIDE"][JSON payload]`

## Implementation Plan

### Phase 1: Convert gateway.rs to server

**File: `src/gateway.rs`**

Replace `GatewayClient` with `GatewayServer`:

```rust
pub struct GatewayServer {
    listener: TcpListener,
    port: u16,
    // Channel to send/receive messages with the UI
    tx: Sender<GatewayMessage>,
    rx: Receiver<GatewayCommand>,
}

pub enum GatewayMessage {
    Connected { addr: String },
    Disconnected,
    Output { result: String, type: u8 },
}

pub enum GatewayCommand {
    Execute { text: String, response_tx: Sender<String> },
}
```

Key changes:
- `bind(port)` instead of `connect(host, port)`
- `accept()` loop in a background thread
- Handle handshake from interpreter side
- Forward Execute commands to interpreter, return results

### Phase 2: Handle the handshake correctly

When interpreter connects:
1. Read `SupportedProtocols=2` from interpreter
2. Send `UsingProtocol=2` back
3. Read `["Identify", {...}]` from interpreter
4. Send `["ReplyIdentify", {...}]` back
5. Read `["Connect", {"remoteId":N}]` from interpreter
6. Send `["ReplyConnect", {"remoteId":N, "protocolVersion":2}]` back
7. Mark as connected, notify UI

### Phase 3: Wire up expression evaluation

**File: `src/main.rs`**

When user presses Ctrl+E:
1. Check if interpreter is connected
2. If connected: send `["Execute", {"text": line}]` to interpreter
3. Wait for `["AppendSessionOutput", {...}]` response
4. Display result in results pane
5. If not connected: show "no interpreter connected" message

### Phase 4: Connection management

- Show "waiting for interpreter on port 4502" in status bar
- When interpreter connects: show "connected to <addr>"
- When interpreter disconnects: show "disconnected, waiting..."
- Auto-reconnect: keep listening for new connections

### Phase 5: Update editor.toml

Remove `gateway_executable` and `gateway_args` (no longer needed since stride is the server):

```toml
gateway_host = "127.0.0.1"
gateway_port = 4502
# No gateway_executable needed - interpreters connect to stride
```

### Phase 6: Testing

1. Start stride (listens on 4502)
2. Start Kap: `./kap-jvm-text --ride` (connects to stride)
3. Type `2+2` in stride, press Ctrl+E
4. Verify result `4` appears in results pane

## Files to Modify

| File | Changes |
|------|---------|
| `src/gateway.rs` | Complete rewrite: client → server |
| `src/main.rs` | Update Ctrl+E handler, remove spawn_gateway, add server lifecycle |
| `src/config.rs` | Remove gateway_executable/gateway_args fields |
| `src/ui.rs` | Update status bar for connection state |
| `editor.toml` | Remove gateway_executable/gateway_args |

## Risks

1. **Protocol mismatches**: Kap may expect slightly different message format
2. **Timing**: Handshake must happen in correct order
3. **Threading**: Server runs in background thread, needs safe communication with UI
4. **Multiple interpreters**: v1 supports one connection at a time

## Estimated Effort

- Phase 1-2: ~2 hours (gateway server + handshake)
- Phase 3: ~1 hour (expression evaluation)
- Phase 4: ~1 hour (connection management)
- Phase 5: ~30 min (config cleanup)
- Phase 6: ~1 hour (testing with Kap)

Total: ~5-6 hours
