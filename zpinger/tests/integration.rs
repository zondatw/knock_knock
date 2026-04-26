use std::net::TcpListener;
use std::time::Duration;

use zpinger::Pinger;

fn closed_tcp_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr.to_string()
}

#[test]
fn resolve_returns_at_least_one_address() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let resolved = zpinger::resolve(&addr.to_string());
    assert!(!resolved.is_empty());
}

#[test]
fn resolve_returns_empty_on_unresolvable_input() {
    // No DNS, no panic — empty Vec lets the CLI keep going so the
    // real pinger can surface the actual error.
    let resolved = zpinger::resolve("definitely-not-a-real-host.invalid:1");
    assert!(resolved.is_empty());
}

#[test]
fn tcp_pinger_struct_succeeds() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    zpinger::TcpPinger::new(addr.to_string()).ping().unwrap();
}

#[test]
fn tcp_pinger_via_timed_helper() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let p = zpinger::TcpPinger::new(addr.to_string());
    let elapsed = zpinger::timed(&p).unwrap();
    assert!(elapsed > Duration::from_nanos(0));
}

#[test]
fn tcp_pinger_with_custom_timeout() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let p = zpinger::TcpPinger::new(addr.to_string()).with_timeout(Duration::from_secs(1));
    p.ping().unwrap();
}

#[test]
fn udp_pinger_struct_succeeds() {
    let addr = testserver::start_udp_echo("127.0.0.1:0").unwrap();
    zpinger::UdpPinger::new(addr.to_string()).ping().unwrap();
}

#[test]
fn level4_pingers_usable_as_trait_objects() {
    let tcp_addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let udp_addr = testserver::start_udp_echo("127.0.0.1:0").unwrap();
    let pingers: Vec<Box<dyn Pinger>> = vec![
        Box::new(zpinger::TcpPinger::new(tcp_addr.to_string())),
        Box::new(zpinger::UdpPinger::new(udp_addr.to_string())),
    ];
    for p in &pingers {
        p.ping().unwrap();
    }
}

#[test]
fn tcp_pinger_struct_fails_on_closed_port() {
    let p = zpinger::TcpPinger::new(closed_tcp_addr());
    assert!(p.ping().is_err());
}

#[test]
fn http_pinger_struct_succeeds() {
    let addr = testserver::start_http_ok("127.0.0.1:0").unwrap();
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, format!("{}/x", addr));
    p.ping().unwrap();
}

#[test]
fn http_pinger_all_methods_via_struct() {
    let addr = testserver::start_http_ok("127.0.0.1:0").unwrap();
    let target = format!("{}/x", addr);
    for method in [
        zpinger::HttpMethod::Connect,
        zpinger::HttpMethod::Get,
        zpinger::HttpMethod::Post,
        zpinger::HttpMethod::Put,
        zpinger::HttpMethod::Delete,
        zpinger::HttpMethod::Patch,
    ] {
        zpinger::HttpPinger::new(method, target.clone())
            .ping()
            .unwrap_or_else(|e| panic!("{:?} failed: {}", method, e));
    }
}

#[test]
fn http_pinger_via_timed_helper() {
    let addr = testserver::start_http_ok("127.0.0.1:0").unwrap();
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, format!("{}/x", addr));
    let elapsed = zpinger::timed(&p).unwrap();
    assert!(elapsed > Duration::from_nanos(0));
}

#[test]
fn http_pinger_rejects_https_scheme() {
    // No real connection should be attempted; the pinger should refuse
    // up front based on the scheme.
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, "https://example.com:443/foo");
    let err = p.ping().expect_err("https must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("https"), "unexpected error message: {msg}");
}

#[test]
fn http_pinger_rejects_non_http_response() {
    // Point the HTTP pinger at the TCP echo server. The "response" is
    // an echo of the GET request, which starts with "GET", not
    // "HTTP/", so the response validator should reject it. The old
    // implementation would have returned Ok here because the 404/501
    // substring check happens to miss "GET".
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, format!("{}/x", addr));
    assert!(p.ping().is_err());
}
