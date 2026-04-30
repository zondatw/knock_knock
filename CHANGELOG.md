# Changelog

All notable changes are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning is
semver per crate (`zpinger`, `knockknock`). Every release line tags the
version of each published crate.

## [Unreleased]

## [1.7.0] / zpinger 0.7.0 — 2026-04-30

### Added
- **QUIC pinger** (`zpinger::QuicPinger`). Completes an RFC 9000
  QUIC v1 handshake (UDP + TLS 1.3 + transport parameters + ALPN
  agreement) and reports the time taken. Reports success once the
  handshake finishes; doesn't open HTTP/3 streams on top — the
  point is to isolate the connection-establishment cost the way
  `TlsPinger` does for TCP+TLS, but for the QUIC stack. Default
  ALPN is `h3`; override via `with_alpn`. Built on
  [`quinn`](https://crates.io/crates/quinn) with the
  `runtime-tokio` + `rustls-ring` features so QUIC's TLS layer
  shares the ring crypto provider with the rest of the workspace.
  Schemes accepted: `quic://`, `https://`, or `host:port`. Default
  port 443. CLI: `knockknock quic <endpoint> [--alpn=h3,...]`.
  MCP: `quic_ping`. Feature: `quic`.
- **testserver gains `start_quic_ok`** for the integration tests
  above, plus a `--quic <port>` flag on the binary (default 18013)
  serving a self-signed `localhost` cert with ALPN `h3`. Pings
  against this fixture succeed via `with_tls_config` injection;
  see the integration suite for the canonical wiring.
- **RTSP pinger** (`zpinger::RtspPinger`) — sends an `OPTIONS`
  request (RFC 2326 §10.1) over TCP and validates the
  `RTSP/1.0 200` response. `OPTIONS` is the spec-mandated keepalive
  for RTSP; no media-session state required. `rtsp://` runs over
  TCP/554, `rtsps://` (RFC 7826 §19) over TLS/322. CLI:
  `knockknock rtsp <target>`. MCP: `rtsp_ping`. Feature: `rtsp`.
- **RTMP pinger** (`zpinger::RtmpPinger`) — runs the simple Adobe
  RTMP §5.2.1 handshake (C0+C1 → S0+S1+S2 → C2). Reports completion
  time once the handshake finishes; doesn't go further into AMF
  `connect` negotiation. Useful for live-streaming ingest
  monitoring. `rtmp://` runs over TCP/1935, `rtmps://` over
  TLS/443. CLI: `knockknock rtmp <target>`. MCP: `rtmp_ping`.
  Feature: `rtmp`.
- **testserver gains `start_rtsp_ok` / `start_rtmp_ok`** for the
  integration tests above; the binary exposes them on default
  ports 18011 / 18012.
- **TLS handshake pinger** (`zpinger::TlsPinger`) — TCP connect +
  TLS handshake (ClientHello → ServerHello → Certificate →
  Finished), then close. Reports success when the handshake
  completes; cert validation errors surface as protocol errors.
  Reuses the existing rustls + webpki-roots stack;
  `with_tls_config` overrides for self-signed test endpoints. Use
  this for cert / handshake monitoring without conflating HTTP
  response time. Default port 443. CLI: `knockknock tls <target>`.
  MCP: `tls_ping`. Feature: `tls`.
- **NTP pinger** (`zpinger::NtpPinger`) — sends one 48-byte NTP v4
  client packet (RFC 5905 §7.3) over UDP and validates the server
  reply (mode field, version echo). Default port 123. Hand-rolled
  wire format, no extra deps. CLI: `knockknock ntp <server>`.
  MCP: `ntp_ping`. Feature: `ntp`.
- **STUN pinger** (`zpinger::StunPinger`) — sends one Binding
  Request (RFC 5389 §6) and validates the Binding Success Response
  (message type, magic cookie, transaction ID echo). Default port
  3478. Hand-rolled wire format. CLI: `knockknock stun <server>`.
  MCP: `stun_ping`. Feature: `stun`.
- **TURN pinger** (`zpinger::TurnPinger`) — sends one
  unauthenticated Allocate Request (RFC 5766 §6.1) with the
  REQUESTED-TRANSPORT attribute and treats the expected
  `401 Unauthorized` Allocate Error Response as a successful
  liveness check. The 401 IS the success signal — no actual relay
  state allocated, no credentials needed, safe to spam against
  shared TURN infrastructure. Default port 3478. CLI:
  `knockknock turn <server>`. MCP: `turn_ping`. Feature: `turn`.
- **testserver gains `start_ntp_ok` / `start_stun_ok` /
  `start_turn_ok`** for the integration tests above.

## [1.6.0] / zpinger 0.6.0 — 2026-04-29

### Added
- **Per-protocol Cargo features for `zpinger`.** Each pinger lives
  behind its own feature: `tcp`, `udp`, `dns`, `http`, `ws`, `mqtt`,
  `hls`, `grpc`. `default = ["all"]` preserves pre-0.6 behavior so
  existing users see no change; downstream crates that only need a
  subset can `default-features = false, features = ["tcp", "udp",
  "dns"]` and skip rustls / tonic / tokio-tungstenite entirely. The
  `Pinger` trait, `timed`, `resolve`, and the URI parser are always
  compiled — they are the crate's core surface.
- CI feature matrix job that runs `cargo check -p zpinger
  --no-default-features --features <set>` across ten combinations to
  keep the gates honest.

### Changed
- `zpinger/src/util.rs` extracted out of `level4.rs` so
  `with_timeout` is shared cleanly across feature subsets.
- `GrpcStreamPinger` now uses tonic's native
  `Streaming::message()` instead of `futures_util::StreamExt::next()`,
  so the `grpc` feature no longer pulls in `futures-util`.

## [1.5.0] / zpinger 0.5.0 — 2026-04-29

### Added
- **HLS pinger** (`zpinger::HlsPinger`). Captures realistic
  player-visible startup latency: GET the M3U8 you point at, follow
  the first `EXT-X-STREAM-INF` variant if it's a master playlist,
  then GET the first segment with `Range: bytes=0-0` so the
  time-to-first-byte metric isn't polluted by full segment download.
  All steps fold into the single `time=` the trait reports.
  `https://` reuses the existing rustls + webpki-roots layer;
  `with_tls_config` overrides for self-signed test endpoints.
- **gRPC streaming pinger** (`zpinger::GrpcStreamPinger`). Calls
  `grpc.health.v1.Health/Watch` instead of `Health/Check` and times
  the first `HealthCheckResponse` message (which the spec mandates
  the server send immediately on subscribe). Exposed through
  `knockknock grpc <endpoint> --watch` rather than its own
  subcommand to keep the CLI shape compact.
- **CLI gains `hls <url>` subcommand and a `--watch` flag on `grpc`.**
- **MCP server gains `hls_ping` and `grpc_watch_ping` tools** so the
  AI agent surface stays at parity with the CLI (now nine tools).
- **testserver gains `start_hls_ok`** + `testserver --hls <port>`
  (default 18007) — minimal HLS responder serving
  `/playlist.m3u8`, `/master.m3u8`, and `/segment0.ts` (with proper
  `Range: bytes=0-0` → 206 Partial Content support).
- **`zpinger` README + crates.io metadata** — first published
  README for the library crate; covers the trait, every pinger
  struct, TLS config, `Box<dyn Pinger>` dispatch, and a pointer to
  the CLI / MCP binaries. Cargo manifest gains `description`,
  `keywords`, `categories`, and `readme = "README.md"`.

## [1.4.0] / zpinger 0.4.0 — 2026-04-29

### Added
- **`knockknock-mcp` binary** — Model Context Protocol server
  exposing every pinger as a typed tool over stdio, gated behind the
  new `mcp` feature so default installs stay slim. Built on
  [`rmcp`](https://crates.io/crates/rmcp) 0.6 with `serde` /
  `serde_json` / `schemars` for tool argument schemas. Seven tools:
  `tcp_ping`, `udp_ping`, `http_ping`, `ws_ping`, `dns_ping`,
  `mqtt_ping`, `grpc_ping`. Each returns a JSON `PingReport` with
  per-iteration `elapsed_ms` / `success` / `error` plus a
  `summary` block. Default `count` is 1 (single reachability probe);
  AI agents that want statistical RTT pass an explicit count. Default
  `timeout_ms` is 5000.
- README gains an "MCP server" section with the tool table, return
  shape, and a Claude Desktop wiring snippet.

## [1.3.0] / zpinger 0.4.0 — 2026-04-29

### Added
- **gRPC pinger** (`zpinger::GrpcPinger`). Calls the standard
  [gRPC Health Checking Protocol](https://github.com/grpc/grpc/blob/master/doc/health-checking.md)
  `grpc.health.v1.Health/Check` unary RPC via `tonic` 0.12 and
  `tonic-health`. Validates the response status equals `SERVING`.
  Builder mirrors the other pingers: `::new(endpoint)`,
  `.with_service(name)`, `.with_timeout(d)`,
  `.with_ca_cert(pem_bytes)`. Accepts both `grpc://` / `http://`
  for plaintext H2C and `grpcs://` / `https://` for TLS;
  schemeless host:port defaults to plaintext H2C. The TLS path
  uses tonic's `tls-webpki-roots` feature for the public-CA
  default trust store, with `with_ca_cert` overriding it for
  self-signed test endpoints.
- `knockknock grpc <endpoint> [--service NAME]` subcommand.
- `testserver::start_grpc_ok` (plaintext) and `start_grpcs_ok`
  (TLS, returns the self-signed cert PEM as a trust anchor) plus
  `testserver --grpc <port>` (default 18006). The server is built
  on `tonic-health::server`, runs in a tokio runtime spawned in a
  background thread to keep the testserver lib's surface API sync.

### Changed (BREAKING)
- **`Pinger` trait is now `async`** — `async fn ping(&self) -> Result<()>`
  via the `async-trait` macro for object-safety. All seven existing
  pingers (TCP / UDP / HTTP / HTTPS / WebSocket / WSS / DNS / MQTT v3.1.1
  + v5) migrated to tokio-based I/O: `tokio::net` for sockets,
  `tokio-rustls` for TLS, `tokio-tungstenite` for WebSocket. Per-socket
  read/write timeouts replaced by an overall `tokio::time::timeout`
  wrapper applied at the start of each `ping`.
- **`zpinger::resolve` is now `async`** — uses `tokio::net::lookup_host`
  so it doesn't block the runtime.
- **`zpinger::timed` is now `async`** — `timed(&p).await`.
- The `knockknock` binary's `main` is now `#[tokio::main]`.

This unlocks PR 14b's gRPC pinger via `tonic` without a second
runtime context. testserver's internal threads stay sync — wire-level
protocols are version-neutral, an async client speaks fine to a sync
broker on the same TCP socket.

## [1.2.0] / zpinger 0.3.0 — 2026-04-28

### Added
- **MQTT pinger** (`zpinger::MqttPinger`, `MqttVersion`). Sync,
  zero new external deps, hand-rolled MQTT (CONNECT/CONNACK +
  PINGREQ/PINGRESP + DISCONNECT) with both **MQTT 3.1.1 (default)**
  and **MQTT 5** wire formats. Builder mirrors the other pingers
  (`::new(server)`, `.with_client_id(s)`, `.with_keepalive(n)`,
  `.with_timeout(d)`, `.with_version(MqttVersion::V5)`,
  `.with_tls_config(c)`). Default port 1883 plain, 8883 TLS;
  `mqtts://` reuses the rustls + webpki-roots layer from PR 8.
  Validation: CONNACK has return / reason code 0 (works for both
  versions because the success byte is at the same offset),
  PINGRESP is the right packet type with no payload. v5 CONNECT
  packets emit the mandatory empty Properties section
  (RFC §3.1.2.11).
- `knockknock mqtt <broker> [--client-id ID] [--v5]` subcommand.
- `testserver::start_mqtt_ok` / `start_mqtts_ok` and
  `testserver --mqtt <port>` (default 18005) — minimal in-process
  broker that accepts CONNECT, replies CONNACK rc=0, replies to
  PINGREQ with PINGRESP, exits on DISCONNECT. The TLS variant
  reuses the same self-signed-cert + injected-trust-anchor model
  as `start_https_ok` / `start_wss_ok`.
- **DNS pinger** (`zpinger::DnsPinger`, `RecordType` enum). Sends one
  UDP query (RFC 1035 wire format, hand-rolled — no external DNS
  crate) and validates the response: matching 16-bit ID, QR bit
  set, RCODE = 0, QDCOUNT = 1, and the question section echoed
  byte-for-byte from the request (per RFC 1035 §4.1.2). The "did
  the server reply" probe is intentionally narrower than a full
  resolver — answer record content is not parsed. Supported record
  types: A, AAAA, CNAME, MX, NS, TXT.
- `knockknock dns <server> -q <name> [-t <type>]` subcommand.
  `<server>` accepts bare host (`8.8.8.8`), host:port, or
  schemeless URLs; default port 53.
- `testserver::start_dns_ok` and `testserver --dns <port>` (default
  18004) for end-to-end testing without any external resolver.

### Changed
- `testserver`'s self-signed TLS cert generation is now a single
  internal helper (`make_test_tls_pair`) shared by `start_https_ok`,
  `start_wss_ok`, and `start_mqtts_ok` — previously it was
  duplicated per-protocol.

### Fixed
- DNS subcommand's "DNS lookup:" CLI banner now resolves with the
  scheme-appropriate port 53 instead of the generic default 80.

### Security
- **HIGH** — Bump `regex` to ≥1.5.5 (GHSA-m5pq-gvj9-9vr8, ReDoS).
  Workspace was holding `regex 1.5.4` via Cargo.lock; resolved to
  `regex 1.12.3` after lifting the `regex = "1.1.9"` requirement on
  `zpinger` to `regex = "1"` and running `cargo update -p regex`.
- **LOW** — Drop the unmaintained `atty` transitive dependency
  (GHSA-g98v-hv3f-hcfr, unaligned read; no upstream patch). Achieved by
  upgrading `colored` from `2` to `3` in `knockknock`; `colored 3`
  switched to `std::io::IsTerminal` and no longer pulls `atty`.

## [1.1.0] / zpinger 0.2.0 — 2026-04-27

First release after a full rewrite of the pinger core. Published to
crates.io via the new tag-driven release workflow.

### Added
- `Pinger` trait + `timed` helper exposing a uniform interface for
  every protocol implementation.
- Struct-based pingers: `TcpPinger`, `UdpPinger`, `HttpPinger`
  (with `HttpMethod` enum), `WebSocketPinger`. Each accepts a builder
  config (`with_timeout`, `with_tls_config` where applicable) and is
  callable through `Box<dyn Pinger>`.
- HTTPS support via `rustls` 0.23 + `webpki-roots` 0.26 — pure-Rust
  crypto via the `ring` backend, no system trust store dependency.
- WebSocket (`ws://` and `wss://`) support via `tungstenite` 0.24,
  reusing the same TLS layer; ping flow runs the full RFC 6455
  upgrade plus a control PING/PONG round trip.
- New `testserver` workspace member providing TCP echo, UDP echo,
  HTTP 200-OK, HTTPS 200-OK, plain `ws://`, and TLS `wss://` test
  endpoints. Used by both the integration suite and as a runnable
  binary for manual e2e (`cargo run -p testserver`).
- CLI restructured into subcommands (`tcp`, `udp`, `http <method>`,
  `ws`); HTTPS / WSS are first-class via scheme detection in the
  target URL.
- Pre-commit hooks (fmt, clippy `-D warnings`, basic hygiene) and a
  matching GitHub Actions CI workflow (fmt + clippy + tests on
  ubuntu and macOS).
- Tag-driven release workflow mirroring
  [`magic-pack`](https://github.com/zondatw/magic-pack): GitHub
  Releases with binaries for five targets, plus `cargo publish` of
  every workspace crate without `publish = false`.

### Changed
- CLI surface is no longer `knockknock <target> -p PROTOCOL`; it is
  now a subcommand tree. Old usage no longer parses.
- `zpinger` no longer exposes `tcping` / `udping` / `httping_*` free
  functions or the `PingHandler` HashMap dispatch — the trait is
  the single dispatch path.
- `clap` upgraded from `3.0.0-beta.5` (yaml-based) to `4` with
  derive macros.
- `URI` parser now treats the port segment as optional and falls
  back to scheme defaults (80/443) for HTTP and HTTPS.

### Fixed
- Plain HTTP requests sent to an HTTPS port no longer return Ok by
  accident. The status line is now validated to start with `HTTP/`,
  and `https://` / `wss://` are routed through the TLS layer rather
  than being silently misencoded.
- `resolve()` returns an empty `Vec` instead of panicking when DNS
  fails, so the actual pinger gets a chance to surface the real
  error to the user.
