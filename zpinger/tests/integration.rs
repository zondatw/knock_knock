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

#[tokio::test]
async fn resolve_returns_at_least_one_address() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let resolved = zpinger::resolve(&addr.to_string()).await;
    assert!(!resolved.is_empty());
}

#[tokio::test]
async fn resolve_returns_empty_on_unresolvable_input() {
    // No DNS, no panic — empty Vec lets the CLI keep going so the
    // real pinger can surface the actual error.
    let resolved = zpinger::resolve("definitely-not-a-real-host.invalid:1").await;
    assert!(resolved.is_empty());
}

#[tokio::test]
async fn tcp_pinger_struct_succeeds() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    zpinger::TcpPinger::new(addr.to_string())
        .ping()
        .await
        .unwrap();
}

#[tokio::test]
async fn tcp_pinger_via_timed_helper() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let p = zpinger::TcpPinger::new(addr.to_string());
    let elapsed = zpinger::timed(&p).await.unwrap();
    assert!(elapsed > Duration::from_nanos(0));
}

#[tokio::test]
async fn tcp_pinger_with_custom_timeout() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let p = zpinger::TcpPinger::new(addr.to_string()).with_timeout(Duration::from_secs(1));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn udp_pinger_struct_succeeds() {
    let addr = testserver::start_udp_echo("127.0.0.1:0").unwrap();
    zpinger::UdpPinger::new(addr.to_string())
        .ping()
        .await
        .unwrap();
}

#[tokio::test]
async fn level4_pingers_usable_as_trait_objects() {
    let tcp_addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let udp_addr = testserver::start_udp_echo("127.0.0.1:0").unwrap();
    let pingers: Vec<Box<dyn Pinger>> = vec![
        Box::new(zpinger::TcpPinger::new(tcp_addr.to_string())),
        Box::new(zpinger::UdpPinger::new(udp_addr.to_string())),
    ];
    for p in &pingers {
        p.ping().await.unwrap();
    }
}

#[tokio::test]
async fn tcp_pinger_struct_fails_on_closed_port() {
    let p = zpinger::TcpPinger::new(closed_tcp_addr());
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn http_pinger_struct_succeeds() {
    let addr = testserver::start_http_ok("127.0.0.1:0").unwrap();
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, format!("{}/x", addr));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn http_pinger_all_methods_via_struct() {
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
            .await
            .unwrap_or_else(|e| panic!("{:?} failed: {}", method, e));
    }
}

#[tokio::test]
async fn http_pinger_via_timed_helper() {
    let addr = testserver::start_http_ok("127.0.0.1:0").unwrap();
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, format!("{}/x", addr));
    let elapsed = zpinger::timed(&p).await.unwrap();
    assert!(elapsed > Duration::from_nanos(0));
}

#[tokio::test]
async fn http_pinger_rejects_unknown_scheme() {
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, "ftp://example.com:21/foo");
    let err = p
        .ping()
        .await
        .expect_err("non-http scheme must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("ftp"), "unexpected error message: {msg}");
}

#[tokio::test]
async fn http_pinger_https_succeeds_with_trusted_cert() {
    let server = testserver::start_https_ok("127.0.0.1:0").unwrap();
    let target = format!("https://localhost:{}/anything", server.addr.port());
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, target)
        .with_tls_config(server.client_config);
    p.ping().await.unwrap();
}

#[tokio::test]
async fn http_pinger_https_all_methods_via_struct() {
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
            .await
            .unwrap_or_else(|e| panic!("{method:?} failed: {e}"));
    }
}

#[tokio::test]
async fn http_pinger_succeeds_without_explicit_port_on_localhost_default() {
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
        .await
        .expect_err("port 80 should be refused on this host");
    let msg = err.to_string().to_lowercase();
    assert!(
        !msg.contains("missing host") && !msg.contains("invalid"),
        "unexpected error type for implicit-port path: {msg}"
    );
}

#[tokio::test]
async fn http_pinger_https_fails_without_trust_anchor() {
    let server = testserver::start_https_ok("127.0.0.1:0").unwrap();
    let target = format!("https://localhost:{}/anything", server.addr.port());
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, target);
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn http_pinger_rejects_non_http_response() {
    let addr = testserver::start_tcp_echo("127.0.0.1:0").unwrap();
    let p = zpinger::HttpPinger::new(zpinger::HttpMethod::Get, format!("{}/x", addr));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn ws_pinger_succeeds_on_ws_server() {
    let addr = testserver::start_ws_ok("127.0.0.1:0").unwrap();
    let target = format!("ws://{addr}/");
    zpinger::WebSocketPinger::new(target).ping().await.unwrap();
}

#[tokio::test]
async fn ws_pinger_via_timed_helper() {
    let addr = testserver::start_ws_ok("127.0.0.1:0").unwrap();
    let target = format!("ws://{addr}/");
    let p = zpinger::WebSocketPinger::new(target);
    let elapsed = zpinger::timed(&p).await.unwrap();
    assert!(elapsed > Duration::from_nanos(0));
}

#[tokio::test]
async fn ws_pinger_fails_on_closed_port() {
    let target = format!("ws://{}/", closed_tcp_addr());
    let p = zpinger::WebSocketPinger::new(target);
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn ws_pinger_rejects_non_ws_scheme() {
    let p = zpinger::WebSocketPinger::new("http://example.com:80/");
    let err = p.ping().await.expect_err("non-ws scheme must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("http"), "unexpected error message: {msg}");
}

#[tokio::test]
async fn wss_pinger_succeeds_with_trusted_cert() {
    let server = testserver::start_wss_ok("127.0.0.1:0").unwrap();
    let target = format!("wss://localhost:{}/", server.addr.port());
    let p = zpinger::WebSocketPinger::new(target).with_tls_config(server.client_config);
    p.ping().await.unwrap();
}

#[tokio::test]
async fn wss_pinger_fails_without_trust_anchor() {
    let server = testserver::start_wss_ok("127.0.0.1:0").unwrap();
    let target = format!("wss://localhost:{}/", server.addr.port());
    let p = zpinger::WebSocketPinger::new(target);
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn ws_pinger_usable_as_trait_object() {
    let addr = testserver::start_ws_ok("127.0.0.1:0").unwrap();
    let target = format!("ws://{addr}/");
    let p: Box<dyn Pinger> = Box::new(zpinger::WebSocketPinger::new(target));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn dns_pinger_succeeds_on_test_server() {
    let addr = testserver::start_dns_ok("127.0.0.1:0").unwrap();
    let p = zpinger::DnsPinger::new(addr.to_string(), "example.com");
    p.ping().await.unwrap();
}

#[tokio::test]
async fn dns_pinger_via_timed_helper() {
    let addr = testserver::start_dns_ok("127.0.0.1:0").unwrap();
    let p = zpinger::DnsPinger::new(addr.to_string(), "example.com");
    let elapsed = zpinger::timed(&p).await.unwrap();
    assert!(elapsed > Duration::from_nanos(0));
}

#[tokio::test]
async fn dns_pinger_with_record_type() {
    let addr = testserver::start_dns_ok("127.0.0.1:0").unwrap();
    let p = zpinger::DnsPinger::new(addr.to_string(), "example.com")
        .with_record_type(zpinger::RecordType::Aaaa);
    p.ping().await.unwrap();
}

#[tokio::test]
async fn dns_pinger_rejects_empty_query() {
    let addr = testserver::start_dns_ok("127.0.0.1:0").unwrap();
    let p = zpinger::DnsPinger::new(addr.to_string(), "");
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn dns_pinger_times_out_on_silent_port() {
    // UDP has no "connection refused" — bind a port that never responds
    // and confirm DnsPinger surfaces the timeout instead of hanging.
    let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = silent.local_addr().unwrap();
    let p = zpinger::DnsPinger::new(addr.to_string(), "example.com")
        .with_timeout(Duration::from_millis(150));
    assert!(p.ping().await.is_err());
    drop(silent);
}

#[tokio::test]
async fn dns_pinger_usable_as_trait_object() {
    let addr = testserver::start_dns_ok("127.0.0.1:0").unwrap();
    let p: Box<dyn Pinger> = Box::new(zpinger::DnsPinger::new(addr.to_string(), "example.com"));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn mqtt_pinger_succeeds_against_test_broker() {
    let addr = testserver::start_mqtt_ok("127.0.0.1:0").unwrap();
    let p = zpinger::MqttPinger::new(format!("mqtt://{addr}"));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn mqtt_pinger_via_timed_helper() {
    let addr = testserver::start_mqtt_ok("127.0.0.1:0").unwrap();
    let p = zpinger::MqttPinger::new(format!("mqtt://{addr}"));
    let elapsed = zpinger::timed(&p).await.unwrap();
    assert!(elapsed > Duration::from_nanos(0));
}

#[tokio::test]
async fn mqtt_pinger_with_custom_client_id() {
    let addr = testserver::start_mqtt_ok("127.0.0.1:0").unwrap();
    let p = zpinger::MqttPinger::new(format!("mqtt://{addr}")).with_client_id("custom-test");
    p.ping().await.unwrap();
}

#[tokio::test]
async fn mqtt_pinger_schemeless_uses_plain_path() {
    let addr = testserver::start_mqtt_ok("127.0.0.1:0").unwrap();
    let p = zpinger::MqttPinger::new(addr.to_string());
    p.ping().await.unwrap();
}

#[tokio::test]
async fn mqtt_pinger_rejects_unknown_scheme() {
    let p = zpinger::MqttPinger::new("ftp://example.com:21");
    let err = p
        .ping()
        .await
        .expect_err("non-mqtt scheme must be rejected up front");
    assert!(
        err.to_string().contains("ftp"),
        "unexpected error message: {err}"
    );
}

#[tokio::test]
async fn mqtt_pinger_fails_on_closed_port() {
    let p = zpinger::MqttPinger::new(format!("mqtt://{}", closed_tcp_addr()))
        .with_timeout(Duration::from_millis(500));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn mqtt_pinger_v5_succeeds_against_test_broker() {
    let addr = testserver::start_mqtt_ok("127.0.0.1:0").unwrap();
    let p =
        zpinger::MqttPinger::new(format!("mqtt://{addr}")).with_version(zpinger::MqttVersion::V5);
    p.ping().await.unwrap();
}

#[tokio::test]
async fn mqtts_pinger_v5_succeeds_with_trusted_cert() {
    let server = testserver::start_mqtts_ok("127.0.0.1:0").unwrap();
    let p = zpinger::MqttPinger::new(format!("mqtts://localhost:{}", server.addr.port()))
        .with_version(zpinger::MqttVersion::V5)
        .with_tls_config(server.client_config);
    p.ping().await.unwrap();
}

#[tokio::test]
async fn mqtt_pinger_usable_as_trait_object() {
    let addr = testserver::start_mqtt_ok("127.0.0.1:0").unwrap();
    let p: Box<dyn Pinger> = Box::new(zpinger::MqttPinger::new(format!("mqtt://{addr}")));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn mqtts_pinger_succeeds_with_trusted_cert() {
    let server = testserver::start_mqtts_ok("127.0.0.1:0").unwrap();
    let p = zpinger::MqttPinger::new(format!("mqtts://localhost:{}", server.addr.port()))
        .with_tls_config(server.client_config);
    p.ping().await.unwrap();
}

#[tokio::test]
async fn mqtts_pinger_fails_without_trust_anchor() {
    let server = testserver::start_mqtts_ok("127.0.0.1:0").unwrap();
    let p = zpinger::MqttPinger::new(format!("mqtts://localhost:{}", server.addr.port()))
        .with_timeout(Duration::from_millis(500));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn grpc_pinger_succeeds_against_test_server() {
    let addr = testserver::start_grpc_ok("127.0.0.1:0").unwrap();
    let p = zpinger::GrpcPinger::new(format!("grpc://localhost:{}", addr.port()));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn grpc_pinger_via_timed_helper() {
    let addr = testserver::start_grpc_ok("127.0.0.1:0").unwrap();
    let p = zpinger::GrpcPinger::new(format!("grpc://localhost:{}", addr.port()));
    let elapsed = zpinger::timed(&p).await.unwrap();
    assert!(elapsed > Duration::from_nanos(0));
}

#[tokio::test]
async fn grpc_pinger_with_service_name() {
    let addr = testserver::start_grpc_ok("127.0.0.1:0").unwrap();
    // testserver only marks the overall ("") service as SERVING. A
    // specific service name should come back NOT_SERVING (or
    // unknown), so the ping should fail — proving with_service
    // actually reaches the wire.
    let p = zpinger::GrpcPinger::new(format!("grpc://localhost:{}", addr.port()))
        .with_service("nonexistent.Service");
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn grpc_pinger_fails_on_closed_port() {
    let p = zpinger::GrpcPinger::new(format!("grpc://{}", closed_tcp_addr()))
        .with_timeout(Duration::from_millis(500));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn grpc_pinger_usable_as_trait_object() {
    let addr = testserver::start_grpc_ok("127.0.0.1:0").unwrap();
    let p: Box<dyn Pinger> = Box::new(zpinger::GrpcPinger::new(format!(
        "grpc://localhost:{}",
        addr.port()
    )));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn grpcs_pinger_succeeds_with_trusted_cert() {
    let server = testserver::start_grpcs_ok("127.0.0.1:0").unwrap();
    let p = zpinger::GrpcPinger::new(format!("grpcs://localhost:{}", server.addr.port()))
        .with_ca_cert(server.ca_pem);
    p.ping().await.unwrap();
}

#[tokio::test]
async fn grpcs_pinger_fails_without_trust_anchor() {
    let server = testserver::start_grpcs_ok("127.0.0.1:0").unwrap();
    let p = zpinger::GrpcPinger::new(format!("grpcs://localhost:{}", server.addr.port()))
        .with_timeout(Duration::from_millis(500));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn grpc_stream_pinger_succeeds_against_test_server() {
    let addr = testserver::start_grpc_ok("127.0.0.1:0").unwrap();
    let p = zpinger::GrpcStreamPinger::new(format!("grpc://localhost:{}", addr.port()));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn grpc_stream_pinger_via_timed_helper() {
    let addr = testserver::start_grpc_ok("127.0.0.1:0").unwrap();
    let p = zpinger::GrpcStreamPinger::new(format!("grpc://localhost:{}", addr.port()));
    let elapsed = zpinger::timed(&p).await.unwrap();
    assert!(elapsed > Duration::from_nanos(0));
}

#[tokio::test]
async fn grpc_stream_pinger_fails_on_closed_port() {
    let p = zpinger::GrpcStreamPinger::new(format!("grpc://{}", closed_tcp_addr()))
        .with_timeout(Duration::from_millis(500));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn grpc_stream_pinger_usable_as_trait_object() {
    let addr = testserver::start_grpc_ok("127.0.0.1:0").unwrap();
    let p: Box<dyn Pinger> = Box::new(zpinger::GrpcStreamPinger::new(format!(
        "grpc://localhost:{}",
        addr.port()
    )));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn hls_pinger_succeeds_on_media_playlist() {
    let addr = testserver::start_hls_ok("127.0.0.1:0").unwrap();
    let url = format!("http://localhost:{}/playlist.m3u8", addr.port());
    zpinger::HlsPinger::new(url).ping().await.unwrap();
}

#[tokio::test]
async fn hls_pinger_succeeds_on_master_playlist() {
    // Master playlist points at media playlist via STREAM-INF; the
    // pinger should follow the variant before fetching the segment.
    let addr = testserver::start_hls_ok("127.0.0.1:0").unwrap();
    let url = format!("http://localhost:{}/master.m3u8", addr.port());
    zpinger::HlsPinger::new(url).ping().await.unwrap();
}

#[tokio::test]
async fn hls_pinger_via_timed_helper() {
    let addr = testserver::start_hls_ok("127.0.0.1:0").unwrap();
    let url = format!("http://localhost:{}/playlist.m3u8", addr.port());
    let p = zpinger::HlsPinger::new(url);
    let elapsed = zpinger::timed(&p).await.unwrap();
    assert!(elapsed > Duration::from_nanos(0));
}

#[tokio::test]
async fn hls_pinger_fails_on_non_m3u8_response() {
    // /anything path returns 404 from start_hls_ok; ping should fail
    // because the first GET doesn't yield a playlist.
    let addr = testserver::start_hls_ok("127.0.0.1:0").unwrap();
    let url = format!("http://localhost:{}/not-here.m3u8", addr.port());
    assert!(zpinger::HlsPinger::new(url).ping().await.is_err());
}

#[tokio::test]
async fn hls_pinger_fails_on_closed_port() {
    let url = format!("http://{}/playlist.m3u8", closed_tcp_addr());
    let p = zpinger::HlsPinger::new(url).with_timeout(Duration::from_millis(500));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn hls_pinger_usable_as_trait_object() {
    let addr = testserver::start_hls_ok("127.0.0.1:0").unwrap();
    let url = format!("http://localhost:{}/playlist.m3u8", addr.port());
    let p: Box<dyn Pinger> = Box::new(zpinger::HlsPinger::new(url));
    p.ping().await.unwrap();
}

#[tokio::test]
async fn dns_pinger_rejects_response_with_tampered_question() {
    // Hand-rolled tiny UDP server: receive the query, set QR=1 +
    // RCODE=0 like the real testserver, but corrupt one byte inside
    // the question section before sending the packet back. The
    // pinger's question-echo check must catch this.
    use std::net::UdpSocket;
    use std::thread;

    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    thread::spawn(move || {
        let mut buf = [0u8; 512];
        if let Ok((n, src)) = server.recv_from(&mut buf) {
            if n >= 14 {
                buf[2] |= 0x80;
                buf[3] &= 0xF0;
                buf[13] ^= 0xFF;
                let _ = server.send_to(&buf[..n], src);
            }
        }
    });

    let p = zpinger::DnsPinger::new(addr.to_string(), "example.com")
        .with_timeout(Duration::from_millis(500));
    let err = p
        .ping()
        .await
        .expect_err("tampered question must be rejected");
    assert!(
        err.to_string().contains("question"),
        "unexpected error message: {err}"
    );
}

// -- TLS handshake pinger -------------------------------------------------

#[tokio::test]
async fn tls_pinger_succeeds_with_trusted_cert() {
    let server = testserver::start_https_ok("127.0.0.1:0").unwrap();
    let target = format!("localhost:{}", server.addr.port());
    zpinger::TlsPinger::new(target)
        .with_tls_config(server.client_config)
        .ping()
        .await
        .unwrap();
}

#[tokio::test]
async fn tls_pinger_fails_without_trust_anchor() {
    let server = testserver::start_https_ok("127.0.0.1:0").unwrap();
    let target = format!("localhost:{}", server.addr.port());
    // No `with_tls_config` → defaults to webpki-roots, which doesn't
    // trust the test cert.
    let err = zpinger::TlsPinger::new(target)
        .with_timeout(Duration::from_secs(2))
        .ping()
        .await
        .expect_err("should fail without trust anchor");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("certificate") || msg.contains("trust") || msg.contains("unknown"),
        "unexpected TLS error: {err}"
    );
}

#[tokio::test]
async fn tls_pinger_fails_on_closed_port() {
    let target = closed_tcp_addr();
    let p = zpinger::TlsPinger::new(target).with_timeout(Duration::from_millis(500));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn tls_pinger_usable_as_trait_object() {
    let server = testserver::start_https_ok("127.0.0.1:0").unwrap();
    let target = format!("localhost:{}", server.addr.port());
    let p: Box<dyn Pinger> =
        Box::new(zpinger::TlsPinger::new(target).with_tls_config(server.client_config));
    p.ping().await.unwrap();
}

// -- NTP pinger -----------------------------------------------------------

#[tokio::test]
async fn ntp_pinger_succeeds() {
    let addr = testserver::start_ntp_ok("127.0.0.1:0").unwrap();
    zpinger::NtpPinger::new(addr.to_string())
        .ping()
        .await
        .unwrap();
}

#[tokio::test]
async fn ntp_pinger_with_custom_timeout() {
    let addr = testserver::start_ntp_ok("127.0.0.1:0").unwrap();
    zpinger::NtpPinger::new(addr.to_string())
        .with_timeout(Duration::from_secs(1))
        .ping()
        .await
        .unwrap();
}

#[tokio::test]
async fn ntp_pinger_times_out_on_silent_port() {
    use std::net::UdpSocket;
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let p = zpinger::NtpPinger::new(addr.to_string()).with_timeout(Duration::from_millis(200));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn ntp_pinger_usable_as_trait_object() {
    let addr = testserver::start_ntp_ok("127.0.0.1:0").unwrap();
    let p: Box<dyn Pinger> = Box::new(zpinger::NtpPinger::new(addr.to_string()));
    p.ping().await.unwrap();
}

// -- STUN pinger ----------------------------------------------------------

#[tokio::test]
async fn stun_pinger_succeeds() {
    let addr = testserver::start_stun_ok("127.0.0.1:0").unwrap();
    zpinger::StunPinger::new(addr.to_string())
        .ping()
        .await
        .unwrap();
}

#[tokio::test]
async fn stun_pinger_with_custom_timeout() {
    let addr = testserver::start_stun_ok("127.0.0.1:0").unwrap();
    zpinger::StunPinger::new(addr.to_string())
        .with_timeout(Duration::from_secs(1))
        .ping()
        .await
        .unwrap();
}

#[tokio::test]
async fn stun_pinger_times_out_on_silent_port() {
    use std::net::UdpSocket;
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let p = zpinger::StunPinger::new(addr.to_string()).with_timeout(Duration::from_millis(200));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn stun_pinger_usable_as_trait_object() {
    let addr = testserver::start_stun_ok("127.0.0.1:0").unwrap();
    let p: Box<dyn Pinger> = Box::new(zpinger::StunPinger::new(addr.to_string()));
    p.ping().await.unwrap();
}

// -- TURN pinger ----------------------------------------------------------

#[tokio::test]
async fn turn_pinger_succeeds_on_401_response() {
    let addr = testserver::start_turn_ok("127.0.0.1:0").unwrap();
    zpinger::TurnPinger::new(addr.to_string())
        .ping()
        .await
        .unwrap();
}

#[tokio::test]
async fn turn_pinger_with_custom_timeout() {
    let addr = testserver::start_turn_ok("127.0.0.1:0").unwrap();
    zpinger::TurnPinger::new(addr.to_string())
        .with_timeout(Duration::from_secs(1))
        .ping()
        .await
        .unwrap();
}

#[tokio::test]
async fn turn_pinger_times_out_on_silent_port() {
    use std::net::UdpSocket;
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let p = zpinger::TurnPinger::new(addr.to_string()).with_timeout(Duration::from_millis(200));
    assert!(p.ping().await.is_err());
}

#[tokio::test]
async fn turn_pinger_usable_as_trait_object() {
    let addr = testserver::start_turn_ok("127.0.0.1:0").unwrap();
    let p: Box<dyn Pinger> = Box::new(zpinger::TurnPinger::new(addr.to_string()));
    p.ping().await.unwrap();
}
