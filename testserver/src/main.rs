use clap::Parser;
use std::io::Result;
use std::net::SocketAddr;
use std::thread;

#[derive(Parser)]
#[command(
    name = "testserver",
    about = "Local TCP / UDP / HTTP / WebSocket servers for exercising knockknock"
)]
struct Args {
    /// TCP echo port (use 0 for an OS-picked ephemeral port)
    #[arg(long, default_value_t = 18000)]
    tcp: u16,

    /// UDP echo port (use 0 for an OS-picked ephemeral port)
    #[arg(long, default_value_t = 18001)]
    udp: u16,

    /// HTTP 200-OK port (use 0 for an OS-picked ephemeral port)
    #[arg(long, default_value_t = 18002)]
    http: u16,

    /// WebSocket (ws://) PING-replier port (use 0 for ephemeral)
    #[arg(long, default_value_t = 18003)]
    ws: u16,

    /// DNS responder port (use 0 for ephemeral)
    #[arg(long, default_value_t = 18004)]
    dns: u16,

    /// Bind address (default 0.0.0.0; use 127.0.0.1 for loopback only)
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,
}

fn start_or_die<F>(label: &str, requested_port: u16, f: F) -> SocketAddr
where
    F: FnOnce() -> Result<SocketAddr>,
{
    f().unwrap_or_else(|e| {
        eprintln!(
            "[{label}] failed to bind port {requested_port}: {e}\n\
             hint: pass --{label} <PORT> (or 0 for an ephemeral port)"
        );
        std::process::exit(1);
    })
}

fn main() {
    let args = Args::parse();
    let bind = args.bind.as_str();

    let tcp = start_or_die("tcp", args.tcp, || {
        testserver::start_tcp_echo(format!("{bind}:{}", args.tcp))
    });
    let udp = start_or_die("udp", args.udp, || {
        testserver::start_udp_echo(format!("{bind}:{}", args.udp))
    });
    let http = start_or_die("http", args.http, || {
        testserver::start_http_ok(format!("{bind}:{}", args.http))
    });
    let ws = start_or_die("ws", args.ws, || {
        testserver::start_ws_ok(format!("{bind}:{}", args.ws))
    });
    let dns = start_or_die("dns", args.dns, || {
        testserver::start_dns_ok(format!("{bind}:{}", args.dns))
    });

    println!("[tcp]  listening on {tcp}");
    println!("[udp]  listening on {udp}");
    println!("[http] listening on {http}");
    println!("[ws]   listening on {ws}");
    println!("[dns]  listening on {dns}");
    println!();
    println!("Try in another terminal:");
    println!("  knockknock tcp localhost:{}", tcp.port());
    println!("  knockknock udp localhost:{}", udp.port());
    println!("  knockknock http get localhost:{}/anything", http.port());
    println!("  knockknock ws ws://localhost:{}/", ws.port());
    println!("  knockknock dns 127.0.0.1:{} -q example.com", dns.port());
    println!();
    println!("Press Ctrl+C to stop.");

    loop {
        thread::park();
    }
}
