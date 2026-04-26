use std::net::TcpListener;
use std::sync::Arc;
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
fn http_pinger_rejects_unknown_scheme() {
    // Anything that isn't http or https should be refused up front.
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, "ftp://example.com:21/foo");
    let err = p.ping().expect_err("non-http scheme must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("ftp"), "unexpected error message: {msg}");
}

#[test]
fn http_pinger_https_succeeds_with_trusted_cert() {
    let server = testserver::start_https_ok("127.0.0.1:0").unwrap();
    let target = format!("https://localhost:{}/anything", server.addr.port());
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, target)
        .with_tls_config(server.client_config);
    p.ping().unwrap();
}

#[test]
fn http_pinger_https_all_methods_via_struct() {
    let server = testserver::start_https_ok("127.0.0.1:0").unwrap();
    let target = format!("https://localhost:{}/x", server.addr.port());
    for method in [
        zpinger::HttpMethod::Connect,
        zpinger::HttpMethod::Get,
        zpinger::HttpMethod::Post,
        zpinger::HttpMethod::Put,
        zpinger::HttpMethod::Delete,
        zpinger::HttpMethod::Patch,
    ] {
        zpinger::HttpPinger::new(method, target.clone())
            .with_tls_config(Arc::clone(&server.client_config))
            .ping()
            .unwrap_or_else(|e| panic!("{method:?} failed: {e}"));
    }
}

#[test]
fn http_pinger_succeeds_without_explicit_port_on_localhost_default() {
    // Run an HTTP server on the platform's HTTP default port (80) is
    // not portable in tests, so instead we verify the URI parser +
    // pinger handle the implicit-port path: when the URL has no
    // ":port" segment, the pinger must apply the scheme default and
    // not crash. Use a closed default port — the test asserts that
    // we get a connect-level error (port refused) rather than a
    // "missing host" or parser error.
    let target = "http://127.0.0.1/anything";
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, target);
    let err = p
        .ping()
        .expect_err("port 80 should be refused on this host");
    let msg = err.to_string().to_lowercase();
    assert!(
        !msg.contains("missing host") && !msg.contains("invalid"),
        "unexpected error type for implicit-port path: {msg}"
    );
}

#[test]
fn http_pinger_https_fails_without_trust_anchor() {
    // Without injecting the test server's cert the default trust
    // store (webpki-roots, public CAs only) cannot verify the
    // self-signed cert, so the handshake must fail rather than
    // silently succeed.
    let server = testserver::start_https_ok("127.0.0.1:0").unwrap();
    let target = format!("https://localhost:{}/anything", server.addr.port());
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, target);
    assert!(p.ping().is_err());
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
