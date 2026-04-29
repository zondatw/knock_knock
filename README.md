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
| `grpc`     | `grpc://` / `http://` plaintext, `grpcs://` / `https://` TLS | `grpc.health.v1.Health/Check` unary RPC                |

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

Try in another terminal:
  knockknock tcp localhost:18000
  knockknock udp localhost:18001
  knockknock http get localhost:18002/anything
  knockknock ws ws://localhost:18003/
  knockknock dns 127.0.0.1:18004 -q example.com
  knockknock mqtt mqtt://localhost:18005
  knockknock grpc grpc://localhost:18006
```

If the default ports are taken, override them (use `0` for an OS-picked
ephemeral port, or pass any specific number):

```shell
$ cargo run -p testserver -- --tcp 0 --udp 0 --http 0 --ws 0 --dns 0 --mqtt 0 --grpc 0 --bind 127.0.0.1
```

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
  grpc  gRPC ping — calls the standard
        grpc.health.v1.Health/Check unary RPC. Accepts grpc:// /
        http:// (plaintext H2C) or grpcs:// / https:// (TLS).

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
[gRPC Health Checking Protocol](https://github.com/grpc/grpc/blob/master/doc/health-checking.md)
`grpc.health.v1.Health/Check` unary RPC. The pinger reports success
only when the server returns `SERVING`.

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

Then ask the agent things like "Is `https://api.example.com/health`
reachable?" or "What's the gRPC RTT to my staging service?" and it
will call the right tool.

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
