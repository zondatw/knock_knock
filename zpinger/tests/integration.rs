use std::net::TcpListener;

fn closed_tcp_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr.to_string()
}

#[test]
fn tcping_succeeds_on_echo_server() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    zpinger::tcping(&addr.to_string()).unwrap();
}

#[test]
fn tcping_fails_on_closed_port() {
    assert!(zpinger::tcping(&closed_tcp_addr()).is_err());
}

#[test]
fn udping_succeeds_on_echo_server() {
    let addr = testserver::start_udp_echo("127.0.0.1:0").unwrap();
    zpinger::udping(&addr.to_string()).unwrap();
}

#[test]
fn httping_all_methods_succeed_on_ok_server() {
    let addr = testserver::start_http_ok("127.0.0.1:0").unwrap();
    let target = format!("{}/anything", addr);
    zpinger::httping_connect(&target).unwrap();
    zpinger::httping_get(&target).unwrap();
    zpinger::httping_post(&target).unwrap();
    zpinger::httping_put(&target).unwrap();
    zpinger::httping_delete(&target).unwrap();
    zpinger::httping_patch(&target).unwrap();
}

#[test]
fn httping_get_fails_on_closed_port() {
    let target = format!("{}/", closed_tcp_addr());
    assert!(zpinger::httping_get(&target).is_err());
}

#[test]
fn resolve_returns_at_least_one_address() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let resolved = zpinger::resolve(&addr.to_string());
    assert!(!resolved.is_empty());
}
