# zpinger

Async, protocol-agnostic latency probe library. Every protocol
implements the same `Pinger` trait — call `ping().await` and get
real round-trip time across the application layer, not just whether
a TCP socket opens.

Powers the [`knockknock`](https://crates.io/crates/knockknock) CLI;
also usable directly from any Rust async application.

## Supported protocols

| Struct             | Schemes                                            | Measures                                                |
| ------------------ | -------------------------------------------------- | ------------------------------------------------------- |
| `TcpPinger`        | `host:port`                                        | TCP connect + 1-byte probe + read                       |
| `UdpPinger`        | `host:port`                                        | UDP send + recv from ephemeral local socket             |
| `HttpPinger`       | `http://`, `https://`                              | Full HTTP/1.1 request + status-line validation          |
| `WebSocketPinger`  | `ws://`, `wss://`                                  | RFC 6455 upgrade + control PING/PONG round trip         |
| `DnsPinger`        | host or host:port (default port 53)                | UDP query + response validation (ID / QR / RCODE / question echo) |
| `MqttPinger`       | `mqtt://`, `mqtts://` (3.1.1 default; v5 opt-in)   | CONNECT/CONNACK + PINGREQ/PINGRESP + DISCONNECT         |
| `GrpcPinger`       | `grpc://` / `http://` plaintext, `grpcs://` / `https://` TLS | `grpc.health.v1.Health/Check` unary RPC          |
| `TlsPinger`        | `host[:port]` or `https://` (default port 443)      | TCP connect + TLS handshake (no application data)       |
| `NtpPinger`        | host or host:port (default port 123)                | NTP v4 client packet + server-mode response validation  |
| `StunPinger`       | host or host:port (default port 3478)               | UDP Binding Request + Binding Success Response          |
| `TurnPinger`       | host or host:port (default port 3478)               | UDP Allocate Request + expected `401 Unauthorized` reply |
| `RtspPinger`       | `rtsp://`, `rtsps://`                               | TCP + RFC 2326 OPTIONS request + `RTSP/1.0 200` validation |
| `RtmpPinger`       | `rtmp://`, `rtmps://`                               | TCP + Adobe RTMP §5.2.1 simple handshake (C0/C1/S0/S1/S2/C2) |
| `QuicPinger`       | `quic://`, `https://`, or `host:port` (port 443)    | UDP + RFC 9000 QUIC v1 handshake (TLS 1.3 + ALPN agreement) |

TLS for `https://` / `wss://` / `mqtts://` / `grpcs://` is handled by
[`rustls`](https://github.com/rustls/rustls) with the Mozilla root CA
bundle from
[`webpki-roots`](https://github.com/rustls/webpki-roots) — pure Rust,
no system trust store dependency.

## Install

```toml
[dependencies]
zpinger = "0.6"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

`zpinger` requires a tokio runtime — the `Pinger` trait is async, so
your application needs to be async too. Any tokio runtime works
(current-thread or multi-thread).

### Pick only the protocols you need

Every protocol is its own Cargo feature. The default is `all`, which
matches pre-0.6 behavior — upgrade and you keep getting everything.
But if you only want, say, TCP probing in an embedded-style binary,
opt out of the default and pick what you need:

```toml
# TCP / UDP / DNS only — no TLS, no HTTP stack, no tonic.
zpinger = { version = "0.6", default-features = false, features = ["tcp", "udp", "dns"] }
```

```toml
# HTTPS but no gRPC.
zpinger = { version = "0.6", default-features = false, features = ["http"] }
```

| Feature | Pinger struct(s) exposed             | What pulls in                          |
| ------- | ------------------------------------ | -------------------------------------- |
| `tcp`   | `TcpPinger`                          | nothing extra (tokio is always there)  |
| `udp`   | `UdpPinger`                          | nothing extra                          |
| `dns`   | `DnsPinger`, `RecordType`            | nothing extra                          |
| `http`  | `HttpPinger`, `HttpMethod`           | rustls + tokio-rustls + webpki-roots   |
| `ws`    | `WebSocketPinger`                    | http TLS + tokio-tungstenite + futures-util |
| `mqtt`  | `MqttPinger`, `MqttVersion`          | http TLS (shared)                      |
| `hls`   | `HlsPinger`                          | http TLS (shared)                      |
| `grpc`  | `GrpcPinger`, `GrpcStreamPinger`     | tonic + tonic-health + (tonic's own TLS stack) |
| `tls`   | `TlsPinger`                          | http TLS (shared)                      |
| `ntp`   | `NtpPinger`                          | nothing extra                          |
| `stun`  | `StunPinger`                         | nothing extra                          |
| `turn`  | `TurnPinger`                         | nothing extra (shares STUN's packet builder internally) |
| `rtsp`  | `RtspPinger`                         | http TLS (shared) for `rtsps://`       |
| `rtmp`  | `RtmpPinger`                         | http TLS (shared) for `rtmps://`       |
| `quic`  | `QuicPinger`                         | quinn + (its own rustls-ring TLS stack) |
| `all`   | all of the above                     | all of the above                       |

The `Pinger` trait, `timed`, `resolve`, and the URI parser are
always compiled regardless of which features you pick — they're the
crate's core surface.

## Quick start

```rust
use std::time::Duration;
use zpinger::{Pinger, TcpPinger, timed};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let p = TcpPinger::new("example.com:80").with_timeout(Duration::from_secs(2));
    let elapsed = timed(&p).await?;
    println!("TCP RTT: {elapsed:?}");
    Ok(())
}
```

## The `Pinger` trait

```rust
#[async_trait::async_trait]
pub trait Pinger: Send + Sync {
    async fn ping(&self) -> std::io::Result<()>;
}
```

`Ok(())` means the protocol-level exchange completed; `Err` carries
the underlying I/O or protocol error. Use `zpinger::timed(pinger)`
for the elapsed time of a single ping. The trait is object-safe via
[`async-trait`](https://crates.io/crates/async-trait), so
`Box<dyn Pinger>` works for heterogeneous dispatch.

## Per-protocol examples

### TCP

```rust
use zpinger::{Pinger, TcpPinger};

let p = TcpPinger::new("example.com:80");
p.ping().await?;
```

### UDP

```rust
use zpinger::{Pinger, UdpPinger};

let p = UdpPinger::new("203.0.113.1:5353");
p.ping().await?;
```

### HTTP / HTTPS

```rust
use zpinger::{HttpMethod, HttpPinger, Pinger};

// Plain HTTP GET
HttpPinger::new(HttpMethod::Get, "http://example.com/")
    .ping()
    .await?;

// HTTPS POST — TLS handled automatically via the scheme
HttpPinger::new(HttpMethod::Post, "https://api.example.com/v1/echo")
    .ping()
    .await?;
```

### WebSocket / WSS

```rust
use zpinger::{Pinger, WebSocketPinger};

// ws:// runs the RFC 6455 upgrade + a control PING/PONG round trip
WebSocketPinger::new("ws://localhost:8080/")
    .ping()
    .await?;

// wss:// reuses the rustls + webpki-roots TLS stack
WebSocketPinger::new("wss://echo.websocket.events/")
    .ping()
    .await?;
```

### DNS

```rust
use zpinger::{DnsPinger, Pinger, RecordType};

DnsPinger::new("8.8.8.8", "example.com")
    .with_record_type(RecordType::Aaaa)
    .ping()
    .await?;
```

### MQTT (3.1.1 default, MQTT 5 opt-in)

```rust
use zpinger::{MqttPinger, MqttVersion, Pinger};

// Plain mqtt:// (default port 1883), MQTT 3.1.1
MqttPinger::new("mqtt://broker.hivemq.com")
    .ping()
    .await?;

// mqtts:// (default port 8883) with MQTT 5 + custom client id
MqttPinger::new("mqtts://broker.example.com:8883")
    .with_client_id("my-client")
    .with_version(MqttVersion::V5)
    .ping()
    .await?;
```

### gRPC

Calls the standard
[gRPC Health Checking Protocol](https://github.com/grpc/grpc/blob/master/doc/health-checking.md)
`grpc.health.v1.Health/Check` unary RPC. Reports success when the
server returns `SERVING`.

```rust
use zpinger::{GrpcPinger, Pinger};

// Plaintext H2C
GrpcPinger::new("grpc://localhost:50051")
    .ping()
    .await?;

// TLS via webpki-roots default trust
GrpcPinger::new("grpcs://api.example.com:443")
    .with_service("my.package.Service")
    .ping()
    .await?;
```

### TLS handshake only

For monitoring just the TLS handshake (cert validation + ServerHello
+ Finished) without conflating HTTP response time:

```rust
use zpinger::{Pinger, TlsPinger};

TlsPinger::new("api.example.com:443")
    .ping()
    .await?;
```

### NTP / STUN / TURN

UDP infra pingers — all share the same shape (host or host:port,
default to the protocol's well-known port):

```rust
use zpinger::{NtpPinger, Pinger, StunPinger, TurnPinger};

NtpPinger::new("time.cloudflare.com").ping().await?;        // port 123
StunPinger::new("stun.l.google.com:19302").ping().await?;   // port 3478
TurnPinger::new("turn.example.com").ping().await?;          // port 3478
```

`TurnPinger` is the unusual one: it sends an unauthenticated Allocate
Request and considers the expected `401 Unauthorized` Allocate Error
Response a successful liveness check (RFC 5766 §6.2 mandates that
response). No relay state is allocated server-side, so it's safe to
spam against shared TURN infrastructure.

### RTSP

```rust
use zpinger::{Pinger, RtspPinger};

// rtsp:// runs over TCP/554; OPTIONS is the spec-mandated keepalive
RtspPinger::new("rtsp://camera.example.com:554/")
    .ping()
    .await?;

// rtsps:// runs over TLS/322 (RFC 7826), reuses the rustls layer
RtspPinger::new("rtsps://secure-camera.example.com/")
    .ping()
    .await?;
```

### RTMP

```rust
use zpinger::{Pinger, RtmpPinger};

// rtmp:// runs over TCP/1935; just the Adobe §5.2.1 handshake, no
// AMF connect afterwards — that's enough to validate ingest liveness.
RtmpPinger::new("rtmp://ingest.example.com:1935/live")
    .ping()
    .await?;

// rtmps:// runs over TLS/443
RtmpPinger::new("rtmps://secure-ingest.example.com/live")
    .ping()
    .await?;
```

### QUIC

```rust
use zpinger::{Pinger, QuicPinger};

// Plain HTTP/3 endpoint with default ALPN h3
QuicPinger::new("https://www.cloudflare.com")
    .ping()
    .await?;

// Custom ALPN list — useful for non-h3 stacks
QuicPinger::new("quic://relay.example.com:8443")
    .with_alpn(vec![b"hq-29".to_vec()])
    .ping()
    .await?;
```

## Heterogeneous dispatch via `Box<dyn Pinger>`

```rust
use std::time::Duration;
use zpinger::{HttpMethod, HttpPinger, Pinger, TcpPinger, timed};

let pingers: Vec<Box<dyn Pinger>> = vec![
    Box::new(TcpPinger::new("example.com:80")),
    Box::new(HttpPinger::new(HttpMethod::Get, "https://example.com/")),
];

for p in &pingers {
    let elapsed = timed(p.as_ref()).await?;
    println!("RTT: {elapsed:?}");
}
```

## TLS configuration

Every TLS-aware pinger (`HttpPinger`, `WebSocketPinger`,
`MqttPinger`, `GrpcPinger`) ships with a sensible default that
trusts public CAs via `webpki-roots`. For self-signed test
endpoints, inject a custom config:

```rust
use std::sync::Arc;
use zpinger::{ClientConfig, HttpMethod, HttpPinger, Pinger};

// Build whatever rustls ClientConfig you like — e.g. a custom
// trust anchor for a self-signed test endpoint.
let config: Arc<ClientConfig> = build_my_test_config();

HttpPinger::new(HttpMethod::Get, "https://localhost:8443/health")
    .with_tls_config(config)
    .ping()
    .await?;
```

`GrpcPinger` uses `with_ca_cert(pem_bytes)` instead — tonic's TLS
config takes a different shape.

## Timeouts

Every pinger struct has `.with_timeout(Duration)`. The default is 5
seconds. The whole `ping()` call respects the timeout (not just
each I/O op individually) — if your handshake stalls halfway, you
still get the timeout error.

```rust
use std::time::Duration;
use zpinger::{Pinger, TcpPinger};

TcpPinger::new("slow.example.com:80")
    .with_timeout(Duration::from_millis(500))
    .ping()
    .await?;
```

## Resolve helper

For showing what the pinger will actually connect to (the CLI uses
this for the `DNS lookup: ...` banner):

```rust
let addrs = zpinger::resolve("https://example.com").await;
// ↑ defaults to port 443 because of the https scheme.
```

`resolve` returns an empty `Vec` on failure rather than panicking —
the actual pinger surfaces the real error when you call it.

## CLI + MCP

If you want to use the same probes from a shell or from an AI agent
without writing Rust, install the
[`knockknock`](https://crates.io/crates/knockknock) CLI. It also
ships an optional `knockknock-mcp` Model Context Protocol server
behind the `mcp` feature.

## License

MIT — same as `knockknock`. See [LICENSE](../LICENSE).
