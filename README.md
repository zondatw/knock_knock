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

### Ping TCP path

```shell
$ knock_knock localhost:8000 -c 3
DNS lookup: [[::1]:8000, 127.0.0.1:8000]
localhost:8000: time=   0.86718 ms
localhost:8000: fail
localhost:8000: fail
----- statistic -----
total time: 867.183µs
Connect time: 3, recv time: 1 (33%), lose time: 2 (66%)
```

### Ping UDP path

```shel
$ knock_knock localhost:12000 -p UDP
DNS lookup: [[::1]:12000, 127.0.0.1:12000]
localhost:12000: time=   0.90438 ms
localhost:12000: fail
localhost:12000: fail
----- statistic -----
total time: 904.381µs
Connect time: 3, recv time: 1 (33%), lose time: 2 (66%)
```
