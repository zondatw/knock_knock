---
name: knockknock
description: |
  Probes protocol-level reachability and round-trip latency across 16
  protocols: TCP, UDP, DNS, HTTP, HTTPS, WebSocket (ws/wss), MQTT
  (mqtt/mqtts, 3.1.1 + v5), gRPC (Health.Check + Health.Watch), HLS,
  TLS handshake, NTP, STUN, TURN, RTSP (rtsp/rtsps), RTMP (rtmp/rtmps),
  and QUIC / HTTP/3. Activates when the user asks about latency, RTT,
  reachability, "is X alive", TLS cert / handshake timing, ALPN
  agreement, gRPC health, MQTT broker liveness, STUN / TURN / WebRTC
  infra, RTSP / RTMP streaming ingest, HTTP/3 handshake, multi-region
  comparison, or DNS resolver comparison. Each ping measures the real
  protocol-level exchange — not just whether a TCP socket opens.
  Provides an MCP server (`knockknock-mcp`, preferred for agents —
  structured JSON output) and a CLI binary (`knockknock`).
when_to_use: |
  Trigger on phrases like: ping, reachable, alive, healthy, up, down,
  latency, RTT, slow endpoint, slow handshake, cert expiry, cert chain,
  TLS time, ALPN, gRPC health, mqtt broker, websocket alive, dns
  resolver, time server, ntp drift, stun / turn server, webrtc infra,
  rtsp camera, rtmp ingest, live streaming health, http/3, quic
  handshake, compare regions, compare resolvers.
allowed-tools: ["Bash", "Read"]
---

# knockknock — protocol-aware latency probes

Source: <https://github.com/zondatw/knock_knock> · Crates: `zpinger`
(library) + `knockknock` (CLI) + `knockknock-mcp` (MCP server).

## TL;DR

Pick a protocol, ask for a ping, get back per-iteration RTT plus a
summary. **Prefer the MCP server** (returns JSON) when it's wired up;
fall back to the CLI via Bash when it isn't. Every probe times the
*real* protocol exchange — TCP+TLS+request+response, MQTT
CONNECT/CONNACK/PINGREQ/PINGRESP, gRPC Health.Check, etc. Not raw
ICMP. Not a throughput tool.

```jsonc
// Minimal MCP call — every tool follows this shape.
{ "name": "tcp_ping", "arguments": { "target": "example.com:80" } }
// → { "iterations": [{ "elapsed_ms": 5.2, "success": true, ... }, ...],
//     "summary": { "count": 1, "recv": 1, "lose": 0, "lose_pct": 0,
//                   "total_ms": 5.2 } }
```

## When to use

- **Reachability**: "is `host` up?", "is the broker / camera / ingest
  alive?"
- **Latency**: "why is `https://api.foo.com` slow?", "compare DNS RTT
  Cloudflare vs Google", "how long does the TLS handshake take?"
- **Cert / handshake monitoring**: "is the cert chain on `bar.com:443`
  fast?", "ALPN agreement working for our HTTP/3 endpoint?"
- **Service liveness for app protocols**: gRPC Health, MQTT keepalive,
  WebSocket PING/PONG, RTSP OPTIONS, RTMP handshake, QUIC
  connection-establishment.
- **Multi-region / multi-resolver comparisons**: loop the same tool
  against several hosts, compare `summary.total_ms`.

## When NOT to use

- **Raw ICMP `ping`** — knockknock doesn't do ICMP (no privileges, not
  cross-platform). Use the system `ping` binary.
- **Throughput / bandwidth** — knockknock only measures RTT. Use
  `iperf3` or similar.
- **Application-layer issues** — slow SQL queries, slow JS rendering,
  stuck queues. Probes won't reveal those; defer to the app's own
  client.
- **Packet capture / topology discovery** — not the right tool. Use
  `tcpdump` / `traceroute` / `mtr`.

## Installing the binaries

The skill is just markdown — calling out to anything requires the
binaries on PATH. Detect first; install only if missing.

```bash
# 1. Probe what's installed.
command -v knockknock-mcp >/dev/null 2>&1 && echo "MCP server present"
command -v knockknock     >/dev/null 2>&1 && echo "CLI present"
```

If either is missing, surface this install command to the user (don't
just silently run it — installing into the user's cargo prefix is a
side effect):

```bash
# Both binaries via the latest crates.io release. The `mcp` feature
# is what gives you `knockknock-mcp`; without it you get the CLI only.
cargo install knockknock --features mcp
```

If `cargo` itself is missing, point the user at <https://rustup.rs>.
Don't try to install `cargo` automatically.

For the **MCP wiring** (so this skill's preferred channel works), the
user needs to register `knockknock-mcp` with their Claude client. For
Claude Desktop / Claude Code that means adding to the relevant
config:

```jsonc
{
  "mcpServers": {
    "knockknock": { "command": "knockknock-mcp" }
  }
}
```

After adding the entry the user must restart their Claude client. If
the MCP server isn't reachable from the running session, fall back to
CLI invocation via Bash for the current turn and tell the user to wire
MCP before next time.

## Two delivery modes

| | When to pick | How to call |
|---|---|---|
| **MCP** (preferred) | `knockknock-mcp` is wired into the running Claude session — i.e., MCP tools like `tcp_ping`, `quic_ping` appear in the tool list | Call the tool directly. Returns JSON. |
| **CLI fallback** | MCP not wired, but `knockknock` is on PATH | `Bash`: `knockknock <subcommand> <target> -c <count>` — parse the text output |

If neither is available, surface the install command above and stop.

## Tool / protocol cheat sheet

Every MCP tool returns the same JSON shape (`iterations[]` +
`summary`); CLI subcommands map 1:1 to tool names with `_ping`
stripped.

| MCP tool | CLI subcmd | Wire | Default port | What "success" means |
|---|---|---|---|---|
| `tcp_ping` | `tcp` | TCP connect + 1-byte probe + read | per target | TCP open + 1 byte echoed |
| `udp_ping` | `udp` | UDP send + recv | per target | datagram received |
| `dns_ping` | `dns` | UDP query (RFC 1035) + response validation | 53 | matching ID, QR=1, RCODE=0, question echoed |
| `http_ping` | `http <method>` | HTTP/1.1 request + status-line check | scheme | 2xx/3xx (or 4xx for HEAD/OPTIONS — see code) |
| `ws_ping` | `ws` | RFC 6455 upgrade + control PING/PONG | 80/443 | upgrade + PONG with matching payload |
| `mqtt_ping` | `mqtt [--v5]` | CONNECT + CONNACK + PINGREQ + PINGRESP + DISCONNECT | 1883/8883 | full session round trip |
| `grpc_ping` | `grpc` | `grpc.health.v1.Health/Check` unary RPC | 80/443 | response status `SERVING` |
| `grpc_watch_ping` | `grpc --watch` | `Health/Watch` server-streaming, time first message | 80/443 | first SERVING message received |
| `hls_ping` | `hls` | M3U8 fetch (follow variant if master) + first segment `Range: bytes=0-0` | 80/443 | playlist + first segment first byte |
| `tls_ping` | `tls` | TCP connect + TLS handshake (no app data) | 443 | handshake complete (cert validated) |
| `ntp_ping` | `ntp` | RFC 5905 §7.3 client-mode packet | 123 | server-mode reply, version echoed |
| `stun_ping` | `stun` | RFC 5389 Binding Request | 3478 | Binding Success, magic cookie + TXID echoed |
| `turn_ping` | `turn` | RFC 5766 unauthenticated Allocate Request | 3478 | `401 Unauthorized` reply (the spec-mandated success signal — no auth needed) |
| `rtsp_ping` | `rtsp` | RFC 2326 §10.1 OPTIONS | 554 (rtsp) / 322 (rtsps) | `RTSP/1.0 200` |
| `rtmp_ping` | `rtmp` | Adobe RTMP §5.2.1 simple handshake (C0/C1 → S0/S1/S2 → C2) | 1935 (rtmp) / 443 (rtmps) | handshake completes |
| `quic_ping` | `quic [--alpn h3,...]` | RFC 9000 QUIC v1 handshake (TLS 1.3 + ALPN) | 443 | handshake established + ALPN agreed |

Common arguments:
- `count` (MCP) / `-c` (CLI) — number of iterations. MCP default 1
  (single liveness check); CLI default 3.
- `timeout_ms` (MCP) — per-ping timeout in ms. Default 5000. Whole
  ping respects this, not just per-IO op.
- `client_id`, `v5`, `service`, `record_type`, `alpn` — protocol
  specific. See each tool's MCP description.

## Output interpretation

### MCP JSON

```jsonc
{
  "iterations": [
    { "elapsed_ms": 7.83, "success": true,  "error": null },
    { "elapsed_ms": 0.0,  "success": false, "error": "operation timed out" }
  ],
  "summary": { "count": 2, "recv": 1, "lose": 1, "lose_pct": 50, "total_ms": 7.83 }
}
```

- `summary.recv` / `summary.lose` are the headline numbers; report
  these to the user, not the raw iterations unless asked.
- `iterations[].error` carries the underlying I/O / protocol error.
  Common patterns to recognize:
  - `"timed out"` / `"operation timed out"` — exceeded `timeout_ms`.
    Network / firewall issue, or wrong port.
  - `"certificate"` / `"unknown issuer"` / `"trust"` — TLS cert chain
    didn't validate (cert expired, self-signed, wrong SAN).
  - `"no application protocol"` — ALPN mismatch (QUIC pinger and
    server don't agree on h3 / hq-29 / etc.).
  - `"scheme '<x>' is not supported"` — wrong subcommand for the
    URL the user supplied. Switch tools.
  - `"S0 returned version 255"` — RTMP target isn't speaking RTMP
    (probably HTTP on that port).

### CLI text

```text
DNS lookup: [<addrs>]                       # informational
<target>: time= 12.34567 ms                 # per-iteration success
<target>: fail                              # per-iteration failure
----- statistic -----
total time: 36.456ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

Key rule: presence of `(100%)` recv = service alive. Mixed loss
suggests intermittent path issues; 100% loss with 1+ ms RTT is
impossible (means parsing went wrong — re-run).

## Diagnostic decision tree

For ambiguous user queries, narrow down by asking one clarifying
question or by trying multiple tools in sequence:

1. **"Is `host` up?"** — start with `tcp_ping` (broadest reach). If
   that's green but the user mentioned a specific service (HTTPS,
   gRPC, WebSocket), follow up with the protocol-specific tool to
   confirm the service layer is alive too.
2. **"Why is HTTPS slow?"** — chain `tcp_ping` → `tls_ping` →
   `http_ping`. Compare the deltas:
   - TCP fast, TLS slow → cert chain too long, OCSP stapling off, or
     server slow signing.
   - TCP + TLS fast, HTTP slow → app server slow, not network.
   - Everything slow → network path issue, do `traceroute` separately.
3. **"Compare resolvers / regions"** — loop one tool against multiple
   targets (`dns_ping` vs 1.1.1.1 / 8.8.8.8 / 9.9.9.9, or `tls_ping`
   vs us-east / eu-west / ap-southeast endpoints). Report the per-
   target medians side-by-side.
4. **"Streaming infra dead?"** — `rtsp_ping` for cameras / VoD,
   `rtmp_ping` for live ingest (Twitch / YouTube / nginx-rtmp).
5. **"WebRTC infra check"** — `stun_ping` (NAT traversal) +
   `turn_ping` (relay). TURN's "success" is the spec-mandated 401, so
   don't be alarmed by the error code in logs.
6. **"HTTP/3 handshake"** — `quic_ping` with default `alpn=h3`. If it
   fails with "no application protocol" the server doesn't speak h3
   — try `hq-29` or fall back to `tls_ping` on TCP/443.

For deeper recipes covering multi-step monitoring scenarios, see
`recipes.md` in this skill directory.

## Common gotchas

- **IPv6 vs IPv4**: knockknock's QUIC pinger prefers IPv4 when both
  families are returned, because containerized / loopback test
  fixtures usually bind v4-only. For other protocols, the underlying
  `tokio::net::TcpStream::connect` tries both. If the user reports a
  ping to `localhost` failing for QUIC but succeeding for TCP, this
  is the reason — switch the target to `127.0.0.1` explicitly.
- **TLS pointed at non-TLS port**: `tls_ping example.com:80` will fail
  with a TLS error (server sends HTTP, not ServerHello). Failure mode
  is correct, but the error message can confuse the user; suggest
  retrying on the right port.
- **Public TURN servers silently drop unauth Allocate**: `turn_ping`
  works against canonical implementations (e.g., coturn with
  `--lt-cred-mech`) but most cloud TURN providers as DoS protection
  silently swallow unauthenticated Allocates. Result: timeout, not
  401. Tell the user to test against a coturn instance they control.
- **Self-signed cert**: the CLI deliberately doesn't expose
  `--insecure`. For test endpoints with self-signed certs, the user
  must call the library directly via `with_tls_config(...)` — the
  skill can't bypass cert validation from a CLI invocation.
- **Default ports for schemeless UDP**: `ntp host` defaults to 123,
  `stun host` and `turn host` default to 3478. Override by passing
  `host:port` explicitly.
- **gRPC scheme aliases**: `grpc://` ≡ `http://` (plaintext H2C),
  `grpcs://` ≡ `https://` (TLS). Same wire format; the prefix is just
  documentation for the operator.

## Limitations

- No ICMP — out of scope.
- No bandwidth / throughput — RTT only.
- No SCTP, no DCCP, no proprietary protocols.
- TLS validation can't be bypassed from the CLI (cert / SAN errors
  surface as protocol errors and the ping fails). Use
  `with_tls_config` from the library if testing self-signed.

## Further reading

- `recipes.md` (this skill dir) — full diagnostic walkthroughs for 7
  recurring scenarios.
- Repo README — install, build, list of all 16 protocols with example
  invocations: <https://github.com/zondatw/knock_knock>
- `zpinger` crate docs (library mode) —
  <https://docs.rs/zpinger>
