use clap::{Parser, Subcommand};
use colored::*;
use std::collections::HashMap;
use std::io::Result;
use std::time::Duration;

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

fn resolve_protocol(command: &Command) -> (&'static str, &str) {
    match command {
        Command::Tcp { target } => ("TCP", target.as_str()),
        Command::Udp { target } => ("UDP", target.as_str()),
        Command::Http { method } => match method {
            HttpMethod::Connect { target } => ("HTTP-CONNECT", target.as_str()),
            HttpMethod::Get { target } => ("HTTP-GET", target.as_str()),
            HttpMethod::Post { target } => ("HTTP-POST", target.as_str()),
            HttpMethod::Put { target } => ("HTTP-PUT", target.as_str()),
            HttpMethod::Delete { target } => ("HTTP-DELETE", target.as_str()),
            HttpMethod::Patch { target } => ("HTTP-PATCH", target.as_str()),
        },
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (protocol, target) = resolve_protocol(&cli.command);
    let count = cli.count;

    let mut ping_handler = zpinger::PingHandler {
        protocol_map: HashMap::new(),
    };
    ping_handler.add_pinger(String::from("TCP"), zpinger::tcping);
    ping_handler.add_pinger(String::from("UDP"), zpinger::udping);
    ping_handler.add_pinger(String::from("HTTP-CONNECT"), zpinger::httping_connect);
    ping_handler.add_pinger(String::from("HTTP-GET"), zpinger::httping_get);
    ping_handler.add_pinger(String::from("HTTP-POST"), zpinger::httping_post);
    ping_handler.add_pinger(String::from("HTTP-PUT"), zpinger::httping_put);
    ping_handler.add_pinger(String::from("HTTP-DELETE"), zpinger::httping_delete);
    ping_handler.add_pinger(String::from("HTTP-PATCH"), zpinger::httping_patch);

    let server = zpinger::resolve(target);
    println!("DNS lookup: {:?}", server);

    let mut total_time = Duration::new(0, 0);
    let mut lose_count: u64 = 0;
    for _ in 0..count {
        match ping_handler.ping(protocol, target) {
            Ok(elapsed_time) => {
                display_ping_info(target, elapsed_time);
                total_time += elapsed_time;
            }
            Err(_) => {
                lose_count += 1;
                display_ping_fail(target)
            }
        };
    }

    display_statistic(total_time, count, count - lose_count, lose_count);
    Ok(())
}
