use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use std::io::Result;
use std::time::Duration;
use zpinger::{DnsPinger, HttpPinger, Pinger, TcpPinger, UdpPinger, WebSocketPinger};

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

fn target_of(command: &Command) -> &str {
    match command {
        Command::Tcp { target } => target,
        Command::Udp { target } => target,
        Command::Ws { target } => target,
        Command::Dns { server, .. } => server,
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let target = target_of(&cli.command).to_string();
    let count = cli.count;
    let pinger = build_pinger(&cli.command);

    let resolve_target = match &cli.command {
        // DNS subcommand: zpinger::resolve defaults to port 80 for
        // schemeless inputs, but a DNS server's well-known port is 53.
        // Patch the target so the "DNS lookup:" display shows the
        // address the pinger will actually talk to.
        Command::Dns { server, .. } => {
            let uri = zpinger::uri::get_uri(server);
            if uri.port == 0 && !uri.domain.is_empty() {
                format!("{}:53", uri.domain)
            } else {
                server.clone()
            }
        }
        _ => target.clone(),
    };
    let server = zpinger::resolve(&resolve_target);
    println!("DNS lookup: {:?}", server);

    let mut total_time = Duration::new(0, 0);
    let mut lose_count: u64 = 0;
    for _ in 0..count {
        match zpinger::timed(pinger.as_ref()) {
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
}
