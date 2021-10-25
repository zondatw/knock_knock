use clap::{load_yaml, App};
use colored::*;
use pinger;
use std::collections::HashMap;
use std::io::Result;
use std::time::Duration;

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

fn main() -> Result<()> {
    // init function map
    let mut ping_handler = pinger::PingHandler {
        protocol_map: HashMap::new(),
    };
    ping_handler.add_pinger(String::from("TCP"), pinger::tcping);
    ping_handler.add_pinger(String::from("UDP"), pinger::udping);
    ping_handler.add_pinger(String::from("HTTP"), pinger::httping);

    // load cli config
    let yaml = load_yaml!("cli.yaml");
    let args = App::from(yaml).get_matches();

    // parse args
    let target = args.value_of("Domain").unwrap();
    let protocol = args.value_of("Protocol").unwrap();
    let count = args
        .value_of("Count")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap();

    // DNS resolve
    let server = pinger::resolve(target);
    println!("DNS lookup: {:?}", server);

    // ping
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

    // statistic
    display_statistic(total_time, count, count - lose_count, lose_count);
    Ok(())
}
