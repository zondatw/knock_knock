# Knock Knock

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

A small companion binary `testserver` provides TCP echo, UDP echo, and a
minimal HTTP 200-OK responder so you can exercise every pinger end-to-end
without depending on external services.

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

The same servers are used by `zpinger`'s integration tests, so
`cargo test` already exercises every protocol against a real socket
without any manual setup.

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

`http` accepts both `http://` and `https://` targets — TLS handshakes
are handled by [`rustls`](https://github.com/rustls/rustls) with the
Mozilla root CA bundle from `webpki-roots`, no system trust store
required:

```shell
$ knockknock http get https://example.com:443/
$ knockknock ws ws://localhost:18003/
$ knockknock ws wss://echo.websocket.events/
$ knockknock dns 8.8.8.8 -q example.com
$ knockknock dns 1.1.1.1 -q example.com -t aaaa
$ knockknock mqtt mqtt://broker.hivemq.com
$ knockknock mqtt mqtt://broker.hivemq.com --v5
$ knockknock mqtt mqtts://broker.example.com:8883 --client-id custom
$ knockknock grpc grpc://localhost:50051
$ knockknock grpc grpcs://api.example.com:443 --service my.Svc
```

### Ping TCP path

```shell
$ knockknock tcp localhost:8000 -c 3
DNS lookup: [[::1]:8000, 127.0.0.1:8000]
localhost:8000: time=   0.86718 ms
localhost:8000: fail
localhost:8000: fail
----- statistic -----
total time: 867.183µs
Connect time: 3, recv time: 1 (33%), lose time: 2 (66%)
```

### Ping UDP path

```shell
$ knockknock udp localhost:12000
DNS lookup: [[::1]:12000, 127.0.0.1:12000]
localhost:12000: time=   0.90438 ms
localhost:12000: fail
localhost:12000: fail
----- statistic -----
total time: 904.381µs
Connect time: 3, recv time: 1 (33%), lose time: 2 (66%)
```

### Ping HTTP path

#### CONNECT

```shell
$ knockknock http connect localhost:8888/haha
DNS lookup: [[::1]:8888, 127.0.0.1:8888]
localhost:8888/haha: time=   2.54041 ms
localhost:8888/haha: time=   2.61254 ms
localhost:8888/haha: time=   3.63613 ms
----- statistic -----
total time: 8.789084ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### GET

```shell
$ knockknock http get localhost:8888/haha
DNS lookup: [[::1]:8888, 127.0.0.1:8888]
localhost:8888/haha: time=   2.54041 ms
localhost:8888/haha: time=   2.61254 ms
localhost:8888/haha: time=   3.63613 ms
----- statistic -----
total time: 8.789084ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### POST

```shell
$ knockknock http post localhost:8888/haha
DNS lookup: [[::1]:8888, 127.0.0.1:8888]
localhost:8888/haha: time=   2.54041 ms
localhost:8888/haha: time=   2.61254 ms
localhost:8888/haha: time=   3.63613 ms
----- statistic -----
total time: 8.789084ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### PUT

```shell
$ knockknock http put localhost:8888/haha
DNS lookup: [[::1]:8888, 127.0.0.1:8888]
localhost:8888/haha: time=   2.54041 ms
localhost:8888/haha: time=   2.61254 ms
localhost:8888/haha: time=   3.63613 ms
----- statistic -----
total time: 8.789084ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### DELETE

```shell
$ knockknock http delete localhost:8888/haha
DNS lookup: [[::1]:8888, 127.0.0.1:8888]
localhost:8888/haha: time=   2.54041 ms
localhost:8888/haha: time=   2.61254 ms
localhost:8888/haha: time=   3.63613 ms
----- statistic -----
total time: 8.789084ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```

#### PATCH

```shell
$ knockknock http patch localhost:8888/haha
DNS lookup: [[::1]:8888, 127.0.0.1:8888]
localhost:8888/haha: time=   2.54041 ms
localhost:8888/haha: time=   2.61254 ms
localhost:8888/haha: time=   3.63613 ms
----- statistic -----
total time: 8.789084ms
Connect time: 3, recv time: 3 (100%), lose time: 0 (0%)
```
