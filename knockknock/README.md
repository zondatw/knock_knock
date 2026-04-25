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
