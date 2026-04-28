# Changelog

All notable changes are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning is
semver per crate (`zpinger`, `knockknock`). Every release line tags the
version of each published crate.

## [Unreleased]

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
