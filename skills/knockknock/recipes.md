# knockknock — diagnostic recipes

Companion to `SKILL.md`. Loaded only when the agent decides a recipe
walkthrough is helpful for the current user query. Each recipe is
self-contained: trigger → tool sequence → expected output shape →
follow-up.

All examples assume the MCP server is wired up. For CLI fallback the
mapping is: `tool_name_ping` → `knockknock <tool_name> <args>`.

## 1. Why is HTTPS slow?

**Trigger**: "API at `https://api.foo.com` got slow this morning".

**Strategy**: peel the layers. TCP → TLS → HTTP. Whichever step's RTT
jumped is where to investigate.

```jsonc
{ "name": "tcp_ping",  "arguments": { "target": "api.foo.com:443", "count": 5 } }
{ "name": "tls_ping",  "arguments": { "target": "api.foo.com:443", "count": 5 } }
{ "name": "http_ping", "arguments": { "target": "https://api.foo.com/", "count": 5 } }
```

**Read-out**:

| Pattern | Likely cause |
|---|---|
| TCP fast, TLS slow, HTTP slow | TLS handshake is the bottleneck — cert chain too long, OCSP stapling disabled, server CPU saturated during handshake |
| TCP fast, TLS fast, HTTP slow | Application backend slow. Network is fine. Hand off to APM / app logs. |
| TCP slow → everything slow | Network path. Run `traceroute` / `mtr` separately. Not knockknock's job. |
| TCP fast, TLS times out | Wrong port, or TLS-terminating proxy down |

## 2. Compare DNS resolvers

**Trigger**: "Are we getting the best DNS latency?"

**Strategy**: same query against 3-5 well-known resolvers; rank by
median RTT.

```jsonc
// For each resolver IP, do:
{ "name": "dns_ping", "arguments": {
    "server": "1.1.1.1", "query": "foo.example.com",
    "record_type": "a",  "count": 5
}}
// Repeat for 8.8.8.8, 9.9.9.9, 208.67.222.222, plus the user's
// company / ISP resolver if they specify one.
```

**Read-out**: median `iterations[].elapsed_ms` per resolver, plus
`summary.lose_pct`. Loss > 0 on a public resolver suggests the user's
egress firewall is dropping UDP/53 — escalate.

**Variants**: pass `record_type: "aaaa"` for IPv6, `"mx"` for mail
infrastructure. Don't probe `txt` / `ns` for latency — they often
return larger responses that artificially inflate timing.

## 3. Multi-region service health

**Trigger**: "Is our service equally responsive from each edge?"

**Strategy**: same protocol-specific tool against per-region hostnames;
report side-by-side.

```jsonc
{ "name": "tls_ping", "arguments": { "target": "us-east.api.foo.com:443", "count": 10 } }
{ "name": "tls_ping", "arguments": { "target": "eu-west.api.foo.com:443", "count": 10 } }
{ "name": "tls_ping", "arguments": { "target": "ap-southeast.api.foo.com:443", "count": 10 } }
```

**Read-out**: report `summary.total_ms / count` (mean) and
`summary.lose_pct` per region. Use `tls_ping` (handshake-only, server
doesn't have to do real work) for a clean network-only baseline; use
`http_ping` if the user wants end-to-end including app response.

## 4. WebRTC infrastructure check

**Trigger**: "Are our STUN / TURN servers healthy?"

**Strategy**: STUN binding + TURN allocate. Both are UDP, default port
3478.

```jsonc
{ "name": "stun_ping", "arguments": { "target": "stun.example.com:3478", "count": 3 } }
{ "name": "turn_ping", "arguments": { "target": "turn.example.com:3478", "count": 3 } }
```

**Read-out**: STUN should reply with Binding Success (sub-50 ms RTT for
nearby servers). TURN should reply with `401 Unauthorized` — that's
the success signal, not a failure (RFC 5766 mandates it for unauth
Allocates). If TURN times out, the server may be configured to
silently drop unauth requests as DoS protection — recommend the user
test against a known-canonical coturn instance with `--lt-cred-mech`
enabled.

## 5. Live-streaming ingest health

**Trigger**: "Is our RTMP ingest dead?", "Why can't OBS connect?"

**Strategy**: handshake against the RTMP endpoint. Don't try to
publish — `rtmp_ping` stops at handshake completion, which is enough
to prove the server speaks RTMP and can accept TCP.

```jsonc
{ "name": "rtmp_ping", "arguments": { "target": "rtmp://ingest.example.com/live", "count": 3 } }
// If the user uses TLS-wrapped RTMP:
{ "name": "rtmp_ping", "arguments": { "target": "rtmps://secure-ingest.example.com/live", "count": 3 } }
```

**Read-out**: handshake completes in <100 ms for a healthy ingest. If
`error: "S0 returned version <X>"` (X != 3), the port has the wrong
protocol — probably HTTP on 1935. If timeout, the server is down or
the path is firewalled.

For RTSP cameras / VoD, swap in `rtsp_ping` with `rtsp://host:554/`
(or `rtsps://host:322/`) — it sends OPTIONS instead of a handshake
and validates `RTSP/1.0 200`.

## 6. HTTP/3 handshake validation

**Trigger**: "Does our edge serve QUIC / HTTP/3 cleanly?"

**Strategy**: QUIC handshake with default ALPN `h3`. Doesn't open
streams — just validates UDP + TLS 1.3 + ALPN agreement.

```jsonc
{ "name": "quic_ping", "arguments": { "target": "https://www.foo.com", "count": 5 } }
// To check a non-h3 ALPN (legacy / experimental):
{ "name": "quic_ping", "arguments": {
    "target": "quic://relay.foo.com:8443",
    "alpn": "hq-29", "count": 3
}}
```

**Read-out**: handshake completes in 1-RTT (~ network RTT + small
crypto cost). If `error: "no application protocol"`, the server
doesn't advertise the ALPN you asked for — try a different one or
fall back to `tls_ping` on TCP/443 to confirm TLS at all.

`error: "Endpoint::client" / "platform"` means the local UDP socket
couldn't bind — usually a sandbox issue, not the server's fault.

## 7. MQTT broker keepalive

**Trigger**: "Is our IoT broker accepting connections?"

**Strategy**: full session round trip — CONNECT, CONNACK, PINGREQ,
PINGRESP, DISCONNECT. Catches problems at every layer of MQTT.

```jsonc
{ "name": "mqtt_ping", "arguments": {
    "broker": "mqtt://broker.foo.com:1883",
    "client_id": "knockknock-probe", "count": 3
}}
// MQTT 5 instead of 3.1.1:
{ "name": "mqtt_ping", "arguments": {
    "broker": "mqtts://secure-broker.foo.com:8883",
    "v5": true, "count": 3
}}
```

**Read-out**: a healthy broker round-trips in 5–50 ms over TCP, 10–80
ms over TLS. CONNACK rejection (return code != 0) surfaces in
`iterations[].error` with the broker's reason. Use `client_id` to
avoid colliding with any production client that might be using the
default.

## Recipe selection

When unsure which recipe the user's query maps to:

| User phrase | Recipe |
|---|---|
| slow API / slow HTTPS / cert latency | #1 |
| DNS slow / compare resolvers | #2 |
| service responsive / multi-region / edge health | #3 |
| webrtc / stun / turn / nat traversal | #4 |
| streaming / ingest / rtmp / rtsp / camera dead | #5 |
| http/3 / quic / alpn | #6 |
| mqtt / iot broker / keepalive | #7 |
| general "is host up?" | start with `tcp_ping`, escalate to a specific recipe based on what the user names next |
