use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use std::io::Result;
use std::time::Duration;
use zpinger::{
    DnsPinger, GrpcPinger, GrpcStreamPinger, HlsPinger, HttpPinger, MqttPinger, MqttVersion,
    NtpPinger, Pinger, QuicPinger, RtmpPinger, RtspPinger, StunPinger, TcpPinger, TlsPinger,
    TurnPinger, UdpPinger, WebSocketPinger,
};

#[derive(Parser)]
#[command(name = "knockknock", version, about = "CLI tool for ping protocols")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// ping times
    #[arg(short, long, default_value_t = 3, global = true)]
    count: u64,
}

#[derive(Subcommand)]
enum Command {
    /// TCP ping
    Tcp { target: String },
    /// UDP ping
    Udp { target: String },
    /// HTTP ping
    Http {
        #[command(subcommand)]
        method: HttpMethod,
    },
    /// WebSocket ping (ws:// or wss://) — runs full upgrade handshake
    /// plus a control PING/PONG round trip.
    Ws { target: String },
    /// DNS ping — sends one UDP query to the resolver and validates
    /// the response.
    Dns {
        /// DNS server (e.g. `8.8.8.8`, `1.1.1.1`, `dns.example.com:53`).
        /// Default port is 53 if not specified.
        server: String,
        /// Domain name to look up (e.g. `example.com`).
        #[arg(short = 'q', long)]
        query: String,
        /// DNS record type.
        #[arg(short = 't', long, value_enum, default_value_t = DnsType::A)]
        record_type: DnsType,
    },
    /// gRPC ping — calls the standard
    /// `grpc.health.v1.Health/Check` unary RPC. Accepts `grpc://` /
    /// `http://` for plaintext H2C and `grpcs://` / `https://` for
    /// TLS. Pass `--watch` to call `Health/Watch` server-streaming
    /// instead and measure time-to-first-status-message.
    Grpc {
        /// gRPC endpoint, e.g. `grpc://localhost:50051` or
        /// `https://api.example.com:443`.
        endpoint: String,
        /// Service name passed in `HealthCheckRequest.service`.
        /// Empty (default) asks for the server's overall health.
        #[arg(long, default_value = "")]
        service: String,
        /// Use server-streaming Health/Watch instead of the unary
        /// Health/Check.
        #[arg(long)]
        watch: bool,
    },
    /// HLS ping — fetches a master / media `.m3u8`, follows a variant
    /// if needed, and times the first segment fetch (Range:
    /// bytes=0-0). Captures realistic player startup latency.
    Hls {
        /// HLS playlist URL, e.g.
        /// `https://example.com/stream/master.m3u8`.
        url: String,
    },
    /// MQTT 3.1.1 ping (mqtt:// or mqtts://). Runs the
    /// CONNECT/CONNACK handshake plus a PINGREQ/PINGRESP control
    /// round trip, then DISCONNECT. Default port 1883 plain, 8883
    /// TLS.
    Mqtt {
        /// MQTT broker (e.g. `mqtt://broker.example.com:1883`,
        /// `mqtts://broker.example.com:8883`, or just
        /// `broker.example.com` for plain MQTT on 1883).
        broker: String,
        /// MQTT client identifier sent in the CONNECT packet.
        /// Defaults to `knockknock-<random>`.
        #[arg(long)]
        client_id: Option<String>,
        /// Speak MQTT 5 instead of the default MQTT 3.1.1.
        #[arg(long)]
        v5: bool,
    },
    /// TLS handshake ping — TCP connect + TLS handshake (ClientHello
    /// → ServerHello → Certificate → Finished), then close. Reports
    /// success when the handshake completes; cert validation errors
    /// surface as protocol errors. Default port 443.
    Tls {
        /// Target host:port or https:// URL, e.g.
        /// `example.com:443` or `https://api.example.com`.
        target: String,
    },
    /// NTP ping — sends one 48-byte NTP v4 client packet
    /// (RFC 5905 §7.3) and validates the server response (mode +
    /// version). Default port 123.
    Ntp {
        /// NTP server, e.g. `time.cloudflare.com` or
        /// `pool.ntp.org:123`.
        server: String,
    },
    /// STUN ping — sends one Binding Request (RFC 5389) and validates
    /// the Binding Success Response. Default port 3478.
    Stun {
        /// STUN server, e.g. `stun.l.google.com:19302`.
        server: String,
    },
    /// TURN ping — sends one unauthenticated Allocate Request
    /// (RFC 5766) and treats the expected `401 Unauthorized` Allocate
    /// Error Response as a successful liveness check. Default port
    /// 3478. No actual relay state allocated, so it's safe to spam.
    Turn {
        /// TURN server, e.g. `turn.example.com:3478`.
        server: String,
    },
    /// RTSP ping — sends an `OPTIONS` request (RFC 2326 §10.1) and
    /// validates the `RTSP/1.0 200` response. `rtsp://` (TCP/554) and
    /// `rtsps://` (TLS/322) both accepted.
    Rtsp {
        /// RTSP target, e.g. `rtsp://host:554/` or `rtsps://host`.
        target: String,
    },
    /// RTMP ping — runs the simple Adobe RTMP §5.2.1 handshake
    /// (C0+C1 → S0+S1+S2 → C2) and reports completion time.
    /// `rtmp://` (TCP/1935) and `rtmps://` (TLS/443) both accepted.
    Rtmp {
        /// RTMP target, e.g. `rtmp://host:1935` or `rtmps://host`.
        target: String,
    },
    /// QUIC ping — completes an RFC 9000 QUIC v1 handshake (UDP +
    /// TLS 1.3 + transport parameters + ALPN) and reports the time
    /// taken. Default ALPN is `h3` (HTTP/3); pass --alpn to override.
    /// `quic://`, `https://`, or schemeless `host:port` all accepted.
    /// Default port 443.
    Quic {
        /// QUIC endpoint, e.g. `quic://host:443` or `example.com`.
        endpoint: String,
        /// ALPN protocol(s) to advertise, comma-separated. Default
        /// is `h3`.
        #[arg(long, default_value = "h3")]
        alpn: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum DnsType {
    A,
    Aaaa,
    Cname,
    Mx,
    Ns,
    Txt,
}

impl From<DnsType> for zpinger::RecordType {
    fn from(value: DnsType) -> Self {
        match value {
            DnsType::A => zpinger::RecordType::A,
            DnsType::Aaaa => zpinger::RecordType::Aaaa,
            DnsType::Cname => zpinger::RecordType::Cname,
            DnsType::Mx => zpinger::RecordType::Mx,
            DnsType::Ns => zpinger::RecordType::Ns,
            DnsType::Txt => zpinger::RecordType::Txt,
        }
    }
}

#[derive(Subcommand)]
enum HttpMethod {
    Connect { target: String },
    Get { target: String },
    Post { target: String },
    Put { target: String },
    Delete { target: String },
    Patch { target: String },
}

fn display_ping_info(target: &str, elapsed_time: Duration) {
    let console_str = format!(
        "{}: time={:>10} ms",
        target,
        format!("{:.5}", elapsed_time.as_secs_f64() * 1000.0)
    );
    println!("{}", console_str.green());
}

fn display_ping_fail(target: &str) {
    let console_str = format!("{}: fail", target);
    println!("{}", console_str.red());
}

fn display_statistic(total_time: Duration, count: u64, recv_count: u64, lose_count: u64) {
    println!("{}", "----- statistic -----".bold());
    println!("total time: {:?}", total_time);
    println!(
        "Connect time: {}, recv time: {} ({}%), lose time: {} ({}%)",
        count,
        recv_count,
        if recv_count == 0 {
            0
        } else {
            recv_count * 100 / count
        },
        lose_count,
        if lose_count == 0 {
            0
        } else {
            lose_count * 100 / count
        }
    );
}

fn default_port_target(target: &str, default_port: u16) -> String {
    let uri = zpinger::uri::get_uri(target);
    if uri.port == 0 && !uri.domain.is_empty() {
        format!("{}:{default_port}", uri.domain)
    } else {
        target.to_string()
    }
}

fn target_of(command: &Command) -> &str {
    match command {
        Command::Tcp { target } => target,
        Command::Udp { target } => target,
        Command::Ws { target } => target,
        Command::Dns { server, .. } => server,
        Command::Mqtt { broker, .. } => broker,
        Command::Grpc { endpoint, .. } => endpoint,
        Command::Hls { url } => url,
        Command::Tls { target } => target,
        Command::Ntp { server } => server,
        Command::Stun { server } => server,
        Command::Turn { server } => server,
        Command::Rtsp { target } => target,
        Command::Rtmp { target } => target,
        Command::Quic { endpoint, .. } => endpoint,
        Command::Http { method } => match method {
            HttpMethod::Connect { target }
            | HttpMethod::Get { target }
            | HttpMethod::Post { target }
            | HttpMethod::Put { target }
            | HttpMethod::Delete { target }
            | HttpMethod::Patch { target } => target,
        },
    }
}

fn build_pinger(command: &Command) -> Box<dyn Pinger> {
    match command {
        Command::Tcp { target } => Box::new(TcpPinger::new(target.clone())),
        Command::Udp { target } => Box::new(UdpPinger::new(target.clone())),
        Command::Ws { target } => Box::new(WebSocketPinger::new(target.clone())),
        Command::Dns {
            server,
            query,
            record_type,
        } => Box::new(
            DnsPinger::new(server.clone(), query.clone()).with_record_type((*record_type).into()),
        ),
        Command::Grpc {
            endpoint,
            service,
            watch,
        } => {
            if *watch {
                Box::new(GrpcStreamPinger::new(endpoint.clone()).with_service(service.clone()))
            } else {
                Box::new(GrpcPinger::new(endpoint.clone()).with_service(service.clone()))
            }
        }
        Command::Hls { url } => Box::new(HlsPinger::new(url.clone())),
        Command::Tls { target } => Box::new(TlsPinger::new(target.clone())),
        Command::Ntp { server } => Box::new(NtpPinger::new(server.clone())),
        Command::Stun { server } => Box::new(StunPinger::new(server.clone())),
        Command::Turn { server } => Box::new(TurnPinger::new(server.clone())),
        Command::Rtsp { target } => Box::new(RtspPinger::new(target.clone())),
        Command::Rtmp { target } => Box::new(RtmpPinger::new(target.clone())),
        Command::Quic { endpoint, alpn } => {
            let alpns: Vec<Vec<u8>> = alpn
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.as_bytes().to_vec())
                .collect();
            let mut p = QuicPinger::new(endpoint.clone());
            if !alpns.is_empty() {
                p = p.with_alpn(alpns);
            }
            Box::new(p)
        }
        Command::Mqtt {
            broker,
            client_id,
            v5,
        } => {
            let mut p = MqttPinger::new(broker.clone());
            if let Some(cid) = client_id {
                p = p.with_client_id(cid.clone());
            }
            if *v5 {
                p = p.with_version(MqttVersion::V5);
            }
            Box::new(p)
        }
        Command::Http { method } => {
            let (m, target) = match method {
                HttpMethod::Connect { target } => (zpinger::HttpMethod::Connect, target),
                HttpMethod::Get { target } => (zpinger::HttpMethod::Get, target),
                HttpMethod::Post { target } => (zpinger::HttpMethod::Post, target),
                HttpMethod::Put { target } => (zpinger::HttpMethod::Put, target),
                HttpMethod::Delete { target } => (zpinger::HttpMethod::Delete, target),
                HttpMethod::Patch { target } => (zpinger::HttpMethod::Patch, target),
            };
            Box::new(HttpPinger::new(m, target.clone()))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let target = target_of(&cli.command).to_string();
    let count = cli.count;
    let pinger = build_pinger(&cli.command);

    let resolve_target = match &cli.command {
        // DNS / MQTT / gRPC subcommands: zpinger::resolve defaults
        // to port 80 for schemeless inputs, which is wrong for these
        // protocols. Patch the target so the "DNS lookup:" display
        // shows the address the pinger will actually talk to.
        Command::Dns { server, .. } => default_port_target(server, 53),
        Command::Mqtt { broker, .. } => {
            let uri = zpinger::uri::get_uri(broker);
            let scheme_default = if uri.scheme.eq_ignore_ascii_case("mqtts") {
                8883
            } else {
                1883
            };
            default_port_target(broker, scheme_default)
        }
        Command::Grpc { endpoint, .. } => {
            // Translate grpc:// → http:// and grpcs:// → https:// so
            // resolve() applies the right port default (80 / 443),
            // matching what tonic does at runtime.
            if let Some(rest) = endpoint.strip_prefix("grpcs://") {
                format!("https://{rest}")
            } else if let Some(rest) = endpoint.strip_prefix("grpc://") {
                format!("http://{rest}")
            } else {
                endpoint.clone()
            }
        }
        // TLS handshake speaks to port 443 by default (same as HTTPS).
        // If the user passed a schemeless host, prepend `https://` so
        // `resolve()` picks the right default port.
        Command::Tls { target } => {
            if target.contains("://") || target.contains(':') {
                target.clone()
            } else {
                format!("https://{target}")
            }
        }
        // NTP / STUN / TURN are UDP-only; resolve() defaults to port 80
        // for schemeless inputs, which is wrong here. Patch the target
        // so the "DNS lookup:" line shows the address the pinger will
        // actually talk to.
        Command::Ntp { server } => default_port_target(server, 123),
        Command::Stun { server } => default_port_target(server, 3478),
        Command::Turn { server } => default_port_target(server, 3478),
        // RTSP / RTMP carry their own scheme. Pick the right default
        // port for the "DNS lookup:" banner so it matches the
        // address the pinger will actually dial.
        Command::Rtsp { target } => {
            let scheme = zpinger::uri::get_uri(target).scheme.to_ascii_lowercase();
            let p = if scheme == "rtsps" { 322 } else { 554 };
            default_port_target(target, p)
        }
        Command::Rtmp { target } => {
            let scheme = zpinger::uri::get_uri(target).scheme.to_ascii_lowercase();
            let p = if scheme == "rtmps" { 443 } else { 1935 };
            default_port_target(target, p)
        }
        // QUIC always lands on 443 by default; if user passed
        // `quic://host` strip the scheme so resolve() picks the right
        // default port via `https://`-equivalent handling.
        Command::Quic { endpoint, .. } => {
            if let Some(rest) = endpoint.strip_prefix("quic://") {
                format!("https://{rest}")
            } else {
                default_port_target(endpoint, 443)
            }
        }
        _ => target.clone(),
    };
    let server = zpinger::resolve(&resolve_target).await;
    println!("DNS lookup: {:?}", server);

    let mut total_time = Duration::new(0, 0);
    let mut lose_count: u64 = 0;
    for _ in 0..count {
        match zpinger::timed(pinger.as_ref()).await {
            Ok(elapsed_time) => {
                display_ping_info(&target, elapsed_time);
                total_time += elapsed_time;
            }
            Err(_) => {
                lose_count += 1;
                display_ping_fail(&target);
            }
        };
    }

    display_statistic(total_time, count, count - lose_count, lose_count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("CLI should parse")
    }

    #[test]
    fn parses_tcp() {
        let cli = parse(&["knockknock", "tcp", "localhost:8000", "-c", "3"]);
        assert!(matches!(cli.command, Command::Tcp { .. }));
        assert_eq!(target_of(&cli.command), "localhost:8000");
        assert_eq!(cli.count, 3);
    }

    #[test]
    fn parses_udp() {
        let cli = parse(&["knockknock", "udp", "localhost:12000"]);
        assert!(matches!(cli.command, Command::Udp { .. }));
        assert_eq!(target_of(&cli.command), "localhost:12000");
    }

    #[test]
    fn parses_all_http_methods() {
        for method in ["connect", "get", "post", "put", "delete", "patch"] {
            let cli = parse(&["knockknock", "http", method, "localhost:8888/haha"]);
            assert!(matches!(cli.command, Command::Http { .. }));
            assert_eq!(target_of(&cli.command), "localhost:8888/haha");
        }
    }

    #[test]
    fn parses_http_get_variant() {
        let cli = parse(&["knockknock", "http", "get", "x:80"]);
        match cli.command {
            Command::Http {
                method: HttpMethod::Get { .. },
            } => {}
            other => panic!(
                "expected http get, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn count_default_is_3() {
        let cli = parse(&["knockknock", "tcp", "localhost:8000"]);
        assert_eq!(cli.count, 3);
    }

    #[test]
    fn count_at_root_position() {
        let cli = parse(&["knockknock", "-c", "5", "tcp", "localhost:8000"]);
        assert_eq!(cli.count, 5);
    }

    #[test]
    fn count_at_subcommand_leaf() {
        let cli = parse(&["knockknock", "tcp", "localhost:8000", "-c", "5"]);
        assert_eq!(cli.count, 5);
    }

    #[test]
    fn count_at_http_method_leaf() {
        let cli = parse(&[
            "knockknock",
            "http",
            "get",
            "localhost:8888/haha",
            "-c",
            "7",
        ]);
        assert_eq!(cli.count, 7);
    }

    #[test]
    fn rejects_missing_subcommand() {
        let result = Cli::try_parse_from(["knockknock"]);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_http_method() {
        let result = Cli::try_parse_from(["knockknock", "http", "trace", "localhost:8888"]);
        assert!(result.is_err());
    }

    #[test]
    fn build_pinger_returns_for_every_command() {
        // Smoke check: every CLI variant produces a Pinger without
        // panicking. We don't call .ping() (no network here) — just
        // verify the dispatch table covers every case.
        let cases: &[&[&str]] = &[
            &["knockknock", "tcp", "localhost:1"],
            &["knockknock", "udp", "localhost:1"],
            &["knockknock", "http", "connect", "localhost:1"],
            &["knockknock", "http", "get", "localhost:1"],
            &["knockknock", "http", "post", "localhost:1"],
            &["knockknock", "http", "put", "localhost:1"],
            &["knockknock", "http", "delete", "localhost:1"],
            &["knockknock", "http", "patch", "localhost:1"],
            &["knockknock", "ws", "ws://localhost:1"],
            &["knockknock", "ws", "wss://localhost:1"],
            &["knockknock", "dns", "127.0.0.1:1", "-q", "example.com"],
            &[
                "knockknock",
                "dns",
                "8.8.8.8",
                "-q",
                "example.com",
                "-t",
                "aaaa",
            ],
            &["knockknock", "mqtt", "mqtt://localhost:1883"],
            &["knockknock", "mqtt", "mqtts://broker.example.com"],
            &[
                "knockknock",
                "mqtt",
                "mqtt://localhost:1883",
                "--client-id",
                "custom",
            ],
            &["knockknock", "mqtt", "mqtt://localhost:1883", "--v5"],
            &["knockknock", "grpc", "grpc://localhost:50051"],
            &["knockknock", "grpc", "grpcs://broker.example.com:443"],
            &[
                "knockknock",
                "grpc",
                "grpc://localhost:50051",
                "--service",
                "my.svc",
            ],
            &["knockknock", "grpc", "grpc://localhost:50051", "--watch"],
            &["knockknock", "hls", "http://localhost:18007/playlist.m3u8"],
        ];
        for args in cases {
            let cli = parse(args);
            let _: Box<dyn Pinger> = build_pinger(&cli.command);
        }
    }

    #[test]
    fn parses_ws_subcommand() {
        let cli = parse(&["knockknock", "ws", "ws://localhost:18000/echo"]);
        assert!(matches!(cli.command, Command::Ws { .. }));
        assert_eq!(target_of(&cli.command), "ws://localhost:18000/echo");
    }

    #[test]
    fn parses_dns_subcommand_default_type() {
        let cli = parse(&["knockknock", "dns", "8.8.8.8", "-q", "example.com"]);
        match &cli.command {
            Command::Dns {
                server,
                query,
                record_type,
            } => {
                assert_eq!(server, "8.8.8.8");
                assert_eq!(query, "example.com");
                assert!(matches!(record_type, DnsType::A));
            }
            other => panic!(
                "expected Dns, got {other:?}",
                other = std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn parses_dns_subcommand_aaaa_type() {
        let cli = parse(&[
            "knockknock",
            "dns",
            "1.1.1.1:5353",
            "-q",
            "example.com",
            "-t",
            "aaaa",
        ]);
        match &cli.command {
            Command::Dns {
                server,
                record_type,
                ..
            } => {
                assert_eq!(server, "1.1.1.1:5353");
                assert!(matches!(record_type, DnsType::Aaaa));
            }
            other => panic!(
                "expected Dns, got {other:?}",
                other = std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn dns_subcommand_requires_query() {
        let result = Cli::try_parse_from(["knockknock", "dns", "8.8.8.8"]);
        assert!(result.is_err());
    }

    #[test]
    fn parses_mqtt_subcommand_default_client_id() {
        let cli = parse(&["knockknock", "mqtt", "mqtt://localhost:1883"]);
        match &cli.command {
            Command::Mqtt {
                broker,
                client_id,
                v5,
            } => {
                assert_eq!(broker, "mqtt://localhost:1883");
                assert!(client_id.is_none());
                assert!(!v5, "default should be MQTT 3.1.1");
            }
            other => panic!("expected Mqtt, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn parses_mqtt_subcommand_with_client_id() {
        let cli = parse(&[
            "knockknock",
            "mqtt",
            "mqtts://broker.example.com:8883",
            "--client-id",
            "test-client",
        ]);
        match &cli.command {
            Command::Mqtt {
                broker, client_id, ..
            } => {
                assert_eq!(broker, "mqtts://broker.example.com:8883");
                assert_eq!(client_id.as_deref(), Some("test-client"));
            }
            other => panic!("expected Mqtt, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn parses_mqtt_subcommand_with_v5_flag() {
        let cli = parse(&["knockknock", "mqtt", "mqtt://broker.example.com", "--v5"]);
        match &cli.command {
            Command::Mqtt { v5, .. } => assert!(v5),
            other => panic!("expected Mqtt, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn mqtt_subcommand_requires_broker() {
        let result = Cli::try_parse_from(["knockknock", "mqtt"]);
        assert!(result.is_err());
    }

    #[test]
    fn parses_grpc_subcommand_default_service() {
        let cli = parse(&["knockknock", "grpc", "grpc://localhost:50051"]);
        match &cli.command {
            Command::Grpc {
                endpoint, service, ..
            } => {
                assert_eq!(endpoint, "grpc://localhost:50051");
                assert!(service.is_empty());
            }
            other => panic!("expected Grpc, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn parses_grpc_subcommand_with_service() {
        let cli = parse(&[
            "knockknock",
            "grpc",
            "grpcs://broker.example.com:443",
            "--service",
            "my.Svc",
        ]);
        match &cli.command {
            Command::Grpc {
                endpoint, service, ..
            } => {
                assert_eq!(endpoint, "grpcs://broker.example.com:443");
                assert_eq!(service, "my.Svc");
            }
            other => panic!("expected Grpc, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grpc_subcommand_requires_endpoint() {
        let result = Cli::try_parse_from(["knockknock", "grpc"]);
        assert!(result.is_err());
    }

    #[test]
    fn parses_grpc_subcommand_with_watch_flag() {
        let cli = parse(&["knockknock", "grpc", "grpc://localhost:50051", "--watch"]);
        match &cli.command {
            Command::Grpc { watch, .. } => assert!(watch),
            other => panic!("expected Grpc, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn parses_hls_subcommand() {
        let cli = parse(&["knockknock", "hls", "http://example.com/master.m3u8"]);
        match &cli.command {
            Command::Hls { url } => assert_eq!(url, "http://example.com/master.m3u8"),
            other => panic!("expected Hls, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn hls_subcommand_requires_url() {
        let result = Cli::try_parse_from(["knockknock", "hls"]);
        assert!(result.is_err());
    }

    #[test]
    fn parses_tls_subcommand() {
        let cli = parse(&["knockknock", "tls", "example.com:443"]);
        match &cli.command {
            Command::Tls { target } => assert_eq!(target, "example.com:443"),
            other => panic!("expected Tls, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn parses_ntp_subcommand() {
        let cli = parse(&["knockknock", "ntp", "time.cloudflare.com"]);
        match &cli.command {
            Command::Ntp { server } => assert_eq!(server, "time.cloudflare.com"),
            other => panic!("expected Ntp, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn parses_stun_subcommand() {
        let cli = parse(&["knockknock", "stun", "stun.l.google.com:19302"]);
        match &cli.command {
            Command::Stun { server } => assert_eq!(server, "stun.l.google.com:19302"),
            other => panic!("expected Stun, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn parses_turn_subcommand() {
        let cli = parse(&["knockknock", "turn", "turn.example.com:3478"]);
        match &cli.command {
            Command::Turn { server } => assert_eq!(server, "turn.example.com:3478"),
            other => panic!("expected Turn, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn batch_a_subcommands_require_target() {
        for cmd in ["tls", "ntp", "stun", "turn"] {
            let result = Cli::try_parse_from(["knockknock", cmd]);
            assert!(result.is_err(), "{cmd} should require a target");
        }
    }

    #[test]
    fn parses_rtsp_subcommand() {
        let cli = parse(&["knockknock", "rtsp", "rtsp://example.com:554/"]);
        match &cli.command {
            Command::Rtsp { target } => assert_eq!(target, "rtsp://example.com:554/"),
            other => panic!("expected Rtsp, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn parses_rtmp_subcommand() {
        let cli = parse(&["knockknock", "rtmp", "rtmp://stream.example.com:1935/live"]);
        match &cli.command {
            Command::Rtmp { target } => {
                assert_eq!(target, "rtmp://stream.example.com:1935/live")
            }
            other => panic!("expected Rtmp, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn batch_b_subcommands_require_target() {
        for cmd in ["rtsp", "rtmp"] {
            let result = Cli::try_parse_from(["knockknock", cmd]);
            assert!(result.is_err(), "{cmd} should require a target");
        }
    }

    #[test]
    fn parses_quic_subcommand() {
        let cli = parse(&["knockknock", "quic", "quic://example.com:443"]);
        match &cli.command {
            Command::Quic { endpoint, alpn } => {
                assert_eq!(endpoint, "quic://example.com:443");
                assert_eq!(alpn, "h3");
            }
            other => panic!("expected Quic, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn parses_quic_with_custom_alpn() {
        let cli = parse(&["knockknock", "quic", "example.com", "--alpn", "h3,hq-29"]);
        match &cli.command {
            Command::Quic { alpn, .. } => assert_eq!(alpn, "h3,hq-29"),
            other => panic!("expected Quic, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn quic_subcommand_requires_endpoint() {
        let result = Cli::try_parse_from(["knockknock", "quic"]);
        assert!(result.is_err());
    }
}
