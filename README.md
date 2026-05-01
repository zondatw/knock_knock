# Knock Knock

A protocol-agnostic latency probe. Measures real round-trip time across
the application layer of every supported protocol — not just whether
a TCP socket opens, but whether the actual handshake / RPC completes.

| Subcommand | Schemes / inputs                                    | What's measured                                                |
| ---------- | --------------------------------------------------- | -------------------------------------------------------------- |
| `tcp`      | `host:port`                                         | TCP connect + 1-byte probe + read                              |
| `udp`      | `host:port`                                         | UDP send + recv from ephemeral local socket                    |
| `http`     | `http://`, `https://` (auto via scheme)             | Full HTTP/1.1 request + status-line validation                 |
| `ws`       | `ws://`, `wss://`                                   | RFC 6455 upgrade + control PING/PONG round trip                |
| `dns`      | host or host:port (default port 53)                 | UDP query + response validation (ID, QR, RCODE, question echo) |
| `mqtt`     | `mqtt://`, `mqtts://` (`--v5` for MQTT 5)           | CONNECT/CONNACK + PINGREQ/PINGRESP + DISCONNECT                |
| `grpc`     | `grpc://` / `http://` plaintext, `grpcs://` / `https://` TLS | `grpc.health.v1.Health/Check` unary RPC (or `Health/Watch` server-stream with `--watch`) |
| `hls`      | `http://` / `https://` URL of an `.m3u8` playlist   | M3U8 fetch (master → variant if needed) + first segment's first byte (Range request) |

TLS for `https` / `wss` / `mqtts` / `grpcs` is handled by
[`rustls`](https://github.com/rustls/rustls) with the Mozilla root CA
bundle from
[`webpki-roots`](https://github.com/rustls/webpki-roots) — pure Rust,
no system trust store dependency.

## Cargo

```shell
// run
$ cargo run
// build
$ cargo build
// build release
$ cargo build --release
// test
$ cargo test
```

## Development setup

Quality gate is enforced via [pre-commit](https://pre-commit.com) hooks
locally and the same checks run on every push / PR via GitHub Actions
([.github/workflows/ci.yml](.github/workflows/ci.yml)).

```shell
# one-time install
$ pip install pre-commit          # or: brew install pre-commit
$ pre-commit install               # installs the pre-commit git hook
$ pre-commit install --hook-type pre-push  # installs the pre-push hook (runs tests)

# run all hooks against the entire repo (e.g. after pulling fresh changes)
$ pre-commit run --all-files
```

Hooks enforced (configured in [.pre-commit-config.yaml](.pre-commit-config.yaml)):

| Stage      | Hook                                                  |
| ---------- | ----------------------------------------------------- |
| pre-commit | trailing whitespace, EOF newline, YAML/TOML lint, merge-conflict / large-file guard, line-ending normalization |
| pre-commit | `cargo fmt --all -- --check`                          |
| pre-commit | `cargo clippy --workspace --all-targets -- -D warnings` |
| pre-push   | `cargo test --workspace`                              |

Clippy is configured to deny all warnings, so any new lint becomes a
build failure both locally and in CI.

## Release

Releases are tag-driven and mirror
[`magic-pack`](https://github.com/zondatw/magic-pack)'s flow. Pushing a
`v*` tag to GitHub triggers
[.github/workflows/release.yaml](.github/workflows/release.yaml) which:

1. creates a GitHub Release for the tag,
2. builds `knockknock` on five targets (linux x64/arm64, macOS
   x64/arm64, windows x64) and uploads the archives to the Release,
3. publishes every workspace crate without `publish = false` to
   [crates.io](https://crates.io) — currently `zpinger` then
   `knockknock`, in dependency order. `testserver` is excluded.

To cut a release:

```shell
# 1. update CHANGELOG.md — promote [Unreleased] to the new version
#    section.
# 2. bump version in zpinger/Cargo.toml and knockknock/Cargo.toml,
#    keeping the path-and-version dep on zpinger in sync.
# 3. commit the bump as its own commit:
#       Bumped version: zpinger X.Y.Z, knockknock A.B.C
# 4. tag and push.
git tag v1.2.0
git push --tags
```

See [CHANGELOG.md](CHANGELOG.md) for release history.

The repo needs a `CARGO_REGISTRY_TOKEN` secret (GitHub Settings →
Secrets and variables → Actions) for the publish step to succeed. The
binary upload jobs only need the default `GITHUB_TOKEN`.

## Local test server

A small companion binary `testserver` provides a local endpoint for
**every** supported protocol so you can exercise every pinger
end-to-end without depending on external services. The same servers
power `zpinger`'s integration suite, so `cargo test` covers every
protocol against a real socket without manual setup.

```shell
$ cargo run -p testserver
[tcp]  listening on 0.0.0.0:18000
[udp]  listening on 0.0.0.0:18001
[http] listening on 0.0.0.0:18002
[ws]   listening on 0.0.0.0:18003
[dns]  listening on 0.0.0.0:18004
[mqtt] listening on 0.0.0.0:18005
[grpc] listening on 0.0.0.0:18006
[hls]  listening on 0.0.0.0:18007
[ntp]  listening on 0.0.0.0:18008
[stun] listening on 0.0.0.0:18009
[turn] listening on 0.0.0.0:18010
[rtsp] listening on 0.0.0.0:18011
[rtmp] listening on 0.0.0.0:18012
[quic] listening on 0.0.0.0:18013 (self-signed cert)

Try in another terminal:
  knockknock tcp localhost:18000
  knockknock udp localhost:18001
  knockknock http get localhost:18002/anything
  knockknock ws ws://localhost:18003/
  knockknock dns 127.0.0.1:18004 -q example.com
  knockknock mqtt mqtt://localhost:18005
  knockknock grpc grpc://localhost:18006
  knockknock grpc grpc://localhost:18006 --watch
  knockknock hls http://localhost:18007/playlist.m3u8
  knockknock ntp localhost:18008
  knockknock stun localhost:18009
  knockknock turn localhost:18010
  knockknock rtsp rtsp://localhost:18011
  knockknock rtmp rtmp://localhost:18012
  # quic uses a self-signed cert — see the QUIC section below for the
  # ClientConfig-injection wiring (CLI flag isn't enough on its own).
  knockknock quic localhost:18013
```

If the default ports are taken, override them (use `0` for an OS-picked
ephemeral port, or pass any specific number):

```shell
$ cargo run -p testserver -- --tcp 0 --udp 0 --http 0 --ws 0 --dns 0 --mqtt 0 --grpc 0 --hls 0 --ntp 0 --stun 0 --turn 0 --rtsp 0 --rtmp 0 --quic 0 --bind 127.0.0.1
```

`testserver` doesn't expose a TLS handshake fixture (the `tls` pinger
is the rare protocol where pointing at a real public endpoint is the
straightforward path — see the [TLS](#tls) section below). For
end-to-end TURN testing against production-grade software see
[TURN](#turn).

## Execution

```shell
$ knockknock <COMMAND> [OPTIONS]

Commands:
  tcp   TCP ping
  udp   UDP ping
  http  HTTP ping (with subcommands: connect, get, post, put, delete, patch)
  ws    WebSocket ping (ws:// or wss://) — full upgrade handshake
        plus a control PING/PONG round trip
  dns   DNS ping (UDP/53 default) — sends one query and validates the
        response
  mqtt  MQTT 3.1.1 (default) or MQTT 5 ping (mqtt:// or mqtts://) —
        runs CONNECT/CONNACK plus a PINGREQ/PINGRESP control round
        trip, then DISCONNECT. Pass --v5 for MQTT 5. Default port
        1883 plain, 8883 TLS.
  grpc  gRPC ping — calls grpc.health.v1.Health/Check unary RPC by
        default; pass --watch to call Health/Watch (server-stream)
        and time the first SERVING message instead. Accepts grpc:// /
        http:// (plaintext H2C) or grpcs:// / https:// (TLS).
  hls   HLS ping — fetches the M3U8 (following a variant if the URL
        is a master playlist), then time-to-first-byte of the first
        segment via a Range: bytes=0-0 request.
  tls   TLS handshake ping — TCP connect + TLS handshake (no
        application data). Default port 443. Reuses rustls +
        webpki-roots; cert validation errors surface as protocol
        errors.
  ntp   NTP ping — sends one 48-byte NTP v4 client packet
        (RFC 5905 §7.3) and validates the server reply. Default
        port 123.
  stun  STUN ping — sends one Binding Request (RFC 5389) and
        validates the Binding Success Response. Default port 3478.
  turn  TURN ping — sends one unauthenticated Allocate Request
        (RFC 5766) and treats the expected `401 Unauthorized` reply
        as a successful liveness check. No relay state allocated,
        no credentials needed. Default port 3478.
  rtsp  RTSP ping — sends an OPTIONS request (RFC 2326 §10.1) and
        validates the RTSP/1.0 200 response. rtsp:// (TCP/554) and
        rtsps:// (TLS/322) both accepted.
  rtmp  RTMP ping — runs the simple Adobe RTMP §5.2.1 handshake
        (C0+C1 → S0+S1+S2 → C2). rtmp:// (TCP/1935) and rtmps://
        (TLS/443) both accepted. Useful for live-streaming ingest
        monitoring.
  quic  QUIC ping — completes an RFC 9000 v1 handshake (UDP +
        TLS 1.3 + transport parameters + ALPN agreement) and
        reports the time taken. Default port 443, default ALPN
        h3. quic://, https://, or schemeless host:port accepted.

Options:
  -c, --count <COUNT>  ping times [default: 3]
```

Output shape is the same across every protocol:

```text
DNS lookup: [...]                       # informational; resolve target → IPs
<target>: time=  X.XXXXX ms              # one line per successful ping
<target>: fail                           # one line per failed ping
----- statistic -----
total time: <sum of successes>
Connect time: N, recv time: M (X%), lose time: K (Y%)
```

`time=` is what was actually measured: full handshake + payload exchange
for each protocol, from the moment `ping()` is called to the moment
the server responds (and, where applicable, the close completes).

### TCP

```shell
$ knockknock tcp localhost:18000 -c 3
DNS lookup: [[::1]:18000, 127.0.0.1:18000]
localhost:18000: time=   0.71271 ms
localhost:18000: time=   0.40504 ms
localhost:18000: time=   0.36213 ms
----- statistic -----
total time: 1.479880ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

### UDP

```shell
$ knockknock udp localhost:18001 -c 3
DNS lookup: [[::1]:18001, 127.0.0.1:18001]
localhost:18001: time=   0.67254 ms
localhost:18001: time=   0.46717 ms
localhost:18001: time=   0.41892 ms
----- statistic -----
total time: 1.558630ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

### HTTP

`http` takes a method subcommand. Schemes:
- `http://` (or no scheme)  → plaintext, default port 80
- `https://`                → TLS, default port 443

#### CONNECT

```shell
$ knockknock http connect localhost:18002/anything
DNS lookup: [[::1]:18002, 127.0.0.1:18002]
localhost:18002/anything: time=   2.54041 ms
localhost:18002/anything: time=   2.61254 ms
localhost:18002/anything: time=   3.63613 ms
----- statistic -----
total time: 8.789084ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### GET

```shell
$ knockknock http get localhost:18002/anything
```

#### POST

```shell
$ knockknock http post localhost:18002/anything
```

#### PUT

```shell
$ knockknock http put localhost:18002/anything
```

#### DELETE

```shell
$ knockknock http delete localhost:18002/anything
```

#### PATCH

```shell
$ knockknock http patch localhost:18002/anything
```

#### HTTPS

Same `http` subcommand, just point at an `https://` URL. TLS handshake
is included in the measured time:

```shell
$ knockknock http get https://www.google.com -c 3
DNS lookup: [142.251.155.119:443, 142.251.157.119:443, ...]
https://www.google.com: time=  97.12188 ms
https://www.google.com: time= 104.15767 ms
https://www.google.com: time= 113.83992 ms
----- statistic -----
total time: 315.119459ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

### WebSocket

`ws` runs the full RFC 6455 upgrade handshake **plus** a control
PING/PONG round trip on each iteration, so `time=` includes both
connection setup and the steady-state frame-layer RTT.

#### ws://

```shell
$ knockknock ws ws://localhost:18003/ -c 3
DNS lookup: [[::1]:18003, 127.0.0.1:18003]
ws://localhost:18003/: time=   4.55317 ms
ws://localhost:18003/: time=   4.35833 ms
ws://localhost:18003/: time=   4.21008 ms
----- statistic -----
total time: 13.121580ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### wss:// (against a public echo server)

```shell
$ knockknock ws wss://echo.websocket.events/ -c 3
```

### DNS

Sends one UDP query (RFC 1035, hand-rolled wire format) to the resolver
and validates: response ID matches, QR bit set, RCODE = 0, QDCOUNT = 1,
question section echoed byte-for-byte from the request. Default port 53.

`-q <name>` is required; `-t <type>` defaults to `a`. Supported types:
`a`, `aaaa`, `cname`, `mx`, `ns`, `txt`.

#### Public resolver (A record)

```shell
$ knockknock dns 8.8.8.8 -q example.com -c 3
DNS lookup: [8.8.8.8:53]
8.8.8.8: time=  17.92217 ms
8.8.8.8: time=  23.98338 ms
8.8.8.8: time=  20.41122 ms
----- statistic -----
total time: 62.316770ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### Different record types

```shell
$ knockknock dns 1.1.1.1 -q example.com -t aaaa
$ knockknock dns 1.1.1.1 -q example.com -t mx
$ knockknock dns 1.1.1.1 -q example.com -t txt
```

#### Custom port (e.g. local resolver)

```shell
$ knockknock dns 127.0.0.1:18004 -q example.com -c 3
```

### MQTT

Runs CONNECT → CONNACK → PINGREQ → PINGRESP → DISCONNECT. The full
session is included in the measured time.

`--v5` switches the wire format to MQTT 5 (sends protocol-level byte 5
plus the mandatory empty Properties section in CONNECT). Default is
MQTT 3.1.1.

`--client-id <id>` overrides the auto-generated client ID.

#### mqtt://

```shell
$ knockknock mqtt mqtt://localhost:18005 -c 3
DNS lookup: [[::1]:18005, 127.0.0.1:18005]
mqtt://localhost:18005: time=   3.86383 ms
mqtt://localhost:18005: time=   3.87321 ms
mqtt://localhost:18005: time=   3.71298 ms
----- statistic -----
total time: 11.450020ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### MQTT 5

```shell
$ knockknock mqtt mqtt://localhost:18005 --v5 -c 3
```

#### mqtts:// (TLS)

```shell
$ knockknock mqtt mqtts://broker.example.com:8883 -c 3
```

#### Custom client id

```shell
$ knockknock mqtt mqtt://broker.hivemq.com --client-id my-client-id
```

### gRPC

Calls the standard
[gRPC Health Checking Protocol](https://github.com/grpc/grpc/blob/master/doc/health-checking.md):

- default — `Health/Check` (unary). Returns immediately with current status.
- `--watch` — `Health/Watch` (server-streaming). Server **must** send
  the current status as its first message per spec, so this measures
  the open-stream-to-first-message RTT instead of the unary
  request/response RTT. Useful when you want to know how fast a
  streaming RPC starts producing data, not just whether the endpoint
  is up.

Both report success only when the first response status is `SERVING`.

Schemes:
- `grpc://`  or `http://`   → plaintext H2C
- `grpcs://` or `https://`  → TLS
- bare `host:port`          → defaults to plaintext H2C

`--service <name>` checks a specific service (default empty = overall
server health).

#### Plaintext (grpc://)

```shell
$ knockknock grpc grpc://localhost:18006 -c 3
DNS lookup: [[::1]:18006, 127.0.0.1:18006]
grpc://localhost:18006: time=   7.00425 ms
grpc://localhost:18006: time=   5.21879 ms
grpc://localhost:18006: time=   6.41346 ms
----- statistic -----
total time: 18.636499ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### TLS (grpcs://)

```shell
$ knockknock grpc grpcs://api.example.com:443 -c 3
```

#### Specific service

```shell
$ knockknock grpc grpc://localhost:18006 --service my.package.Service
```

#### Streaming (Health.Watch)

```shell
$ knockknock grpc grpc://localhost:18006 --watch -c 3
```

### HLS

Captures the player-visible startup latency on an HLS endpoint:

1. `GET` the M3U8 you point at — master or media playlist
2. if it was a master playlist, follow the first `EXT-X-STREAM-INF`
   variant and `GET` that media playlist too
3. `GET` the first segment with `Range: bytes=0-0` — measures
   time-to-first-byte without paying the whole segment download

The single `time=` the pinger reports covers all three (or two)
GETs plus TLS handshake when the URL is `https://`.

#### Media playlist directly

```shell
$ knockknock hls http://localhost:18007/playlist.m3u8 -c 3
DNS lookup: [[::1]:18007, 127.0.0.1:18007]
http://localhost:18007/playlist.m3u8: time=  13.91733 ms
http://localhost:18007/playlist.m3u8: time=  14.05038 ms
http://localhost:18007/playlist.m3u8: time=  13.39263 ms
----- statistic -----
total time: 41.360333ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### Master playlist (variant resolution included)

```shell
$ knockknock hls https://example.com/stream/master.m3u8
```

### TLS

Measures pure TLS handshake time (TCP connect + ClientHello +
ServerHello + Certificate + Finished) without conflating any HTTP /
WebSocket / MQTT / gRPC payload time on top. Useful for cert /
handshake monitoring on load balancers and CDN edges.

`testserver` doesn't expose a TLS fixture — the simplest test path
is pointing at a real public endpoint:

```shell
$ knockknock tls api.github.com:443 -c 3
DNS lookup: [20.27.177.116:443]
api.github.com:443: time=  78.78871 ms
api.github.com:443: time=  80.84967 ms
api.github.com:443: time=  79.88504 ms
----- statistic -----
total time: 239.523416ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

Schemeless host gets port 443 by default; `https://host[:port]` URLs
also work (the URI parser recognises the scheme).

```shell
$ knockknock tls cloudflare.com -c 3
$ knockknock tls https://www.google.com -c 3
```

### NTP

Sends one 48-byte NTP v4 client-mode packet and validates the server
reply (mode field is server-mode, version echoes the client's). This
is a "is the time server alive" probe, not a clock-discipline tool —
timestamps in the reply aren't decoded.

#### Local fixture (testserver)

```shell
$ knockknock ntp localhost:18008 -c 3
DNS lookup: [[::1]:18008, 127.0.0.1:18008]
localhost:18008: time=   0.41850 ms
localhost:18008: time=   0.36462 ms
localhost:18008: time=   0.34117 ms
----- statistic -----
total time: 1.124292ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### Real public time servers

```shell
$ knockknock ntp time.cloudflare.com -c 3
DNS lookup: [162.159.200.123:123, 162.159.200.1:123]
time.cloudflare.com: time=   7.83196 ms
time.cloudflare.com: time=   7.80158 ms
time.cloudflare.com: time=   8.47542 ms
```

`pool.ntp.org`, `time.google.com`, `time.apple.com`, etc. all work the
same way. Default port 123, override with explicit `host:port` if
needed.

### STUN

Sends one Binding Request (RFC 5389 §6) and validates the Binding
Success Response (message type 0x0101, magic cookie unchanged,
transaction ID echoed). Useful for monitoring NAT-traversal infra
serving WebRTC clients.

#### Local fixture (testserver)

```shell
$ knockknock stun localhost:18009 -c 3
DNS lookup: [[::1]:18009, 127.0.0.1:18009]
localhost:18009: time=   0.39800 ms
localhost:18009: time=   0.32088 ms
localhost:18009: time=   0.30217 ms
```

#### Real public STUN servers

```shell
$ knockknock stun stun.l.google.com:19302 -c 3
DNS lookup: [74.125.250.129:19302]
stun.l.google.com:19302: time=  11.44071 ms
stun.l.google.com:19302: time=   9.64979 ms
stun.l.google.com:19302: time=  10.26796 ms

$ knockknock stun stun.cloudflare.com:3478 -c 3
```

Default port is 3478. Google's public STUN runs on 19302 and is the
canonical sanity-check endpoint.

### TURN

Sends one **unauthenticated** Allocate Request (RFC 5766 §6.1) and
treats the expected `401 Unauthorized` Allocate Error Response as a
successful liveness check — that 401 IS the success signal, the spec
mandates it. **No relay state allocated, no credentials needed**, so
it's safe to spam against shared TURN infrastructure.

#### Local fixture (testserver)

```shell
$ knockknock turn localhost:18010 -c 3
DNS lookup: [[::1]:18010, 127.0.0.1:18010]
localhost:18010: time=   0.42283 ms
localhost:18010: time=   0.34688 ms
localhost:18010: time=   0.32504 ms
```

#### End-to-end against real coturn

Public TURN servers in the wild typically silently drop unauthenticated
Allocate requests as DoS protection, so finding one on the public
internet that responds with the spec-mandated 401 is hit-or-miss. To
prove the wire format end-to-end against the canonical TURN
implementation, run a local coturn:

```shell
# macOS
$ brew install coturn

# Run with long-term credential mechanism enabled — that's what makes
# coturn return the spec-mandated 401 to unauthenticated Allocate
# requests, which is exactly what our pinger expects to see.
$ turnserver -p 3478 --lt-cred-mech --user=test:test --realm=test --no-cli -v &

# Now ping it.
$ knockknock turn 127.0.0.1:3478 -c 3
DNS lookup: [127.0.0.1:3478]
127.0.0.1:3478: time=   0.81679 ms
127.0.0.1:3478: time=   0.43821 ms
127.0.0.1:3478: time=   0.42633 ms

$ kill %1
```

`--lt-cred-mech` enables long-term credential auth (so unauth requests
get the 401 we want to see). `--realm` sets the realm name that the
401 must include per RFC 5389 §15.7. `--user` is required by coturn
even though our pinger never authenticates — the credential is just
not exercised.

### RTSP

RTSP's `OPTIONS` method is the spec-mandated keepalive (RFC 2326
§10.1) — every conformant server must accept it and answer with the
list of supported methods, no media-session state required. We send
`OPTIONS rtsp://host:port/ RTSP/1.0` and expect `RTSP/1.0 200`.

Schemes: `rtsp://` (TCP/554) and `rtsps://` (TLS/322 per RFC 7826
§19) both work; the TLS variant reuses the same rustls + webpki-roots
stack as `https`.

#### Local fixture (testserver)

```shell
$ knockknock rtsp rtsp://localhost:18011 -c 3
DNS lookup: [[::1]:18011, 127.0.0.1:18011]
rtsp://localhost:18011: time=   0.71450 ms
rtsp://localhost:18011: time=   0.43808 ms
rtsp://localhost:18011: time=   0.40213 ms
----- statistic -----
total time: 1.554710ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### Real RTSP camera or VoD server

```shell
$ knockknock rtsp rtsp://camera.example.com:554/stream1 -c 3
$ knockknock rtsp rtsps://secure-camera.example.com -c 3
```

### RTMP

Runs the simple Adobe RTMP §5.2.1 handshake (`C0 + C1` →
`S0 + S1 + S2` → `C2`) and reports the time to handshake completion.
Doesn't continue into AMF `connect` negotiation — for liveness, the
handshake closure IS the success signal: it proves the peer speaks
RTMP version 3 and got past TCP / TLS plumbing.

Schemes: `rtmp://` (TCP/1935) and `rtmps://` (TLS/443).

#### Local fixture (testserver)

```shell
$ knockknock rtmp rtmp://localhost:18012 -c 3
DNS lookup: [[::1]:18012, 127.0.0.1:18012]
rtmp://localhost:18012: time=   1.02350 ms
rtmp://localhost:18012: time=   0.55408 ms
rtmp://localhost:18012: time=   0.51125 ms
```

#### Real ingest endpoints

```shell
# Live-streaming ingest (Twitch / YouTube / nginx-rtmp / etc.)
$ knockknock rtmp rtmp://ingest.example.com:1935/live -c 3

# TLS-wrapped variant
$ knockknock rtmp rtmps://secure-ingest.example.com/live -c 3
```

You can also run [nginx-rtmp](https://github.com/arut/nginx-rtmp-module)
locally for end-to-end validation against a real-world RTMP
implementation:

```shell
# macOS: brew install nginx (with rtmp module) or via docker
$ docker run --rm -p 1935:1935 tiangolo/nginx-rtmp &
$ knockknock rtmp rtmp://127.0.0.1:1935 -c 3
```

### QUIC

Completes an RFC 9000 QUIC v1 handshake (UDP + TLS 1.3 + transport
parameters + ALPN agreement) and reports the time taken. Doesn't open
any HTTP/3 streams on top — the point is to isolate
connection-establishment cost the way `tls` does for TCP+TLS, but for
the QUIC stack. Default ALPN is `h3` so most production HTTP/3 servers
accept the handshake; override via `--alpn` for `hq-29` / custom
protocols.

#### Real public HTTP/3 endpoints

QUIC's TLS layer trusts webpki-roots by default, so any internet
endpoint with a real cert works:

```shell
$ knockknock quic https://www.cloudflare.com -c 3
DNS lookup: [104.16.132.229:443, 104.16.133.229:443]
https://www.cloudflare.com: time=  18.34112 ms
https://www.cloudflare.com: time=  16.92208 ms
https://www.cloudflare.com: time=  17.25596 ms
----- statistic -----
total time: 52.519160ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)

$ knockknock quic www.google.com -c 3
$ knockknock quic quic://cloudflare-quic.com:443 -c 3
```

#### Local fixture (testserver)

`testserver`'s `--quic` port serves a freshly minted self-signed cert
with ALPN `h3`. The CLI doesn't expose a `--insecure` / custom-CA
flag (deliberately — production monitoring shouldn't bypass cert
validation), so the canonical local-fixture wiring is the integration
suite at `zpinger/tests/integration.rs::quic_pinger_succeeds_with_trusted_cert`,
which feeds the fixture's `ClientConfig` into `QuicPinger::with_tls_config`.

For end-to-end CLI validation against a real cert chain, point at any
internet HTTP/3 endpoint as shown above. Both Cloudflare and Google
expose stable QUIC v1 + h3 endpoints on port 443.

#### Custom ALPN

```shell
# Force a non-h3 ALPN — useful for testing legacy QUIC stacks.
$ knockknock quic relay.example.com:8443 --alpn hq-29 -c 3

# Multiple ALPNs in preference order (server picks one):
$ knockknock quic example.com --alpn h3,hq-29 -c 3
```

If the server doesn't support any ALPN you advertised the handshake
fails with a `no application protocol` error — that's the
ALPN-mismatch signal.

## MCP server (`knockknock-mcp`)

A second binary, `knockknock-mcp`, exposes every protocol as a typed
[Model Context Protocol](https://modelcontextprotocol.io) tool over
stdio. AI agents (Claude Desktop, MCP-aware editors / IDEs) can call
the same pings the CLI does, with structured JSON results that
include per-iteration timings plus a summary.

The binary is gated behind the `mcp` feature so the default
`knockknock` install stays minimal.

```shell
$ cargo install knockknock --features mcp
# or, from a checkout:
$ cargo build -p knockknock --features mcp --release
$ ./target/release/knockknock-mcp     # speaks MCP JSON-RPC on stdin/stdout
```

Tools exposed (one per protocol; `count` defaults to **1**, not 3
like the CLI — agents usually want a single reachability probe rather
than statistical RTT):

| Tool        | Required args        | Optional args                                               |
| ----------- | -------------------- | ----------------------------------------------------------- |
| `tcp_ping`  | `target`             | `count`, `timeout_ms`                                       |
| `udp_ping`  | `target`             | `count`, `timeout_ms`                                       |
| `http_ping` | `target`             | `method` (get/post/...), `count`, `timeout_ms`              |
| `ws_ping`   | `target`             | `count`, `timeout_ms`                                       |
| `dns_ping`  | `server`, `query`    | `record_type` (a/aaaa/cname/mx/ns/txt), `count`, `timeout_ms` |
| `mqtt_ping` | `broker`             | `client_id`, `v5` (bool), `count`, `timeout_ms`             |
| `grpc_ping` | `endpoint`           | `service`, `count`, `timeout_ms`                            |
| `grpc_watch_ping` | `endpoint`     | `service`, `count`, `timeout_ms` — `Health/Watch` server-stream |
| `hls_ping`  | `url`                | `count`, `timeout_ms`                                       |

Every tool returns the same shape:

```json
{
  "iterations": [
    {"elapsed_ms": 5.234, "success": true},
    {"elapsed_ms": 4.891, "success": true}
  ],
  "summary": {
    "count": 2,
    "recv": 2,
    "lose": 0,
    "lose_pct": 0,
    "total_ms": 10.125
  }
}
```

Failed iterations include an `error` field with the underlying error
message (`{"elapsed_ms": 0.0, "success": false, "error": "..."}`).

### Wiring into Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "knockknock": {
      "command": "/absolute/path/to/knockknock-mcp"
    }
  }
}
```

### Wiring into OpenAI Codex

Codex (the
[OpenAI coding agent](https://github.com/openai/codex)) reads MCP
server configuration from `~/.codex/config.toml` (TOML, not JSON).
Add a `[mcp_servers.<name>]` section:

```toml
[mcp_servers.knockknock]
command = "knockknock-mcp"
```

Codex resolves `command` via `PATH`, so as long as
`cargo install knockknock --features mcp` put the binary somewhere
on your shell's `PATH`, no absolute path is needed. Restart Codex
after editing the file — config is read once at startup.

You can also use the built-in CLI instead of hand-editing:

```shell
codex mcp add knockknock --command knockknock-mcp
```

Project-scoped wiring works the same way — drop a `.codex/config.toml`
at the repo root with the same `[mcp_servers.knockknock]` section
when you want the integration scoped to one repo.

### Trying it out

Once wired into either client, ask the agent things like "Is
`https://api.example.com/health` reachable?" or "What's the gRPC RTT
to my staging service?" and it will call the right tool.

## Skill for AI agents

The repo ships a Claude Code skill at `skills/knockknock/` that
teaches **other** Claude agents (e.g., Claude Code, Claude Desktop)
when to invoke knockknock and how to read its output. It's the
"prompt-side" companion to the MCP server — MCP gives the agent the
tools, the skill teaches it which tool to pick and how to interpret
the result.

Install the skill into Claude's user-global skills directory:

```shell
# clone into a stable location, then symlink — keeps the skill in
# sync if the repo updates
git clone https://github.com/zondatw/knock_knock ~/src/knock_knock
ln -s ~/src/knock_knock/skills/knockknock ~/.claude/skills/knockknock
```

Or copy just the markdown if you don't want a clone:

```shell
mkdir -p ~/.claude/skills/knockknock
curl -L https://raw.githubusercontent.com/zondatw/knock_knock/main/skills/knockknock/SKILL.md \
  -o ~/.claude/skills/knockknock/SKILL.md
curl -L https://raw.githubusercontent.com/zondatw/knock_knock/main/skills/knockknock/recipes.md \
  -o ~/.claude/skills/knockknock/recipes.md
```

After installation the agent automatically discovers the skill via
its `description` field; no config edit needed. The skill then
detects whether `knockknock-mcp` / `knockknock` are on PATH and falls
back to surfacing the `cargo install knockknock --features mcp`
command if they aren't.

## Library usage (`zpinger`)

`zpinger` is published on crates.io and exposes the same protocols as a
Rust library. The single contract is the async `Pinger` trait:

```rust
use std::time::Duration;
use zpinger::{HttpMethod, HttpPinger, Pinger, TcpPinger, timed};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // TCP — measure connect + 1-byte probe RTT.
    let p = TcpPinger::new("example.com:80").with_timeout(Duration::from_secs(2));
    let elapsed = timed(&p).await?;
    println!("TCP RTT: {elapsed:?}");

    // HTTP GET — full request + response RTT, https path uses webpki-roots.
    let p = HttpPinger::new(HttpMethod::Get, "https://example.com:443/");
    let elapsed = timed(&p).await?;
    println!("HTTPS GET RTT: {elapsed:?}");

    // Heterogeneous dispatch via Box<dyn Pinger>.
    let pingers: Vec<Box<dyn Pinger>> = vec![
        Box::new(TcpPinger::new("example.com:80")),
        Box::new(HttpPinger::new(HttpMethod::Get, "https://example.com:443/")),
    ];
    for p in &pingers {
        let _ = timed(p.as_ref()).await;
    }

    Ok(())
}
```

Every pinger struct (`TcpPinger`, `UdpPinger`, `HttpPinger`,
`WebSocketPinger`, `DnsPinger`, `MqttPinger`, `GrpcPinger`) follows the
same `::new(target).with_*(opts)` builder shape. See each module's
docs and the integration tests in `zpinger/tests/integration.rs` for
worked examples.
