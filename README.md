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

## Local test server

A small companion binary `testserver` provides TCP echo, UDP echo, and a
minimal HTTP 200-OK responder so you can exercise every pinger end-to-end
without depending on external services.

```shell
$ cargo run -p testserver
[tcp]  listening on 0.0.0.0:18000
[udp]  listening on 0.0.0.0:18001
[http] listening on 0.0.0.0:18002

Try in another terminal:
  knockknock tcp localhost:18000
  knockknock udp localhost:18001
  knockknock http get localhost:18002/anything
```

If the default ports are taken, override them (use `0` for an OS-picked
ephemeral port, or pass any specific number):

```shell
$ cargo run -p testserver -- --tcp 0 --udp 0 --http 0 --bind 127.0.0.1
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

Options:
  -c, --count <COUNT>  ping times [default: 3]
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
