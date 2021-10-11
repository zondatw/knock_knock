use std::io::prelude::*;
use std::io::Result;
use std::net::{TcpStream, ToSocketAddrs, SocketAddr};
use std::time::{Duration, Instant};
use clap::{App, load_yaml};
use colored::*;

fn resolve(domain: &str) -> Vec<SocketAddr> {
   domain.to_socket_addrs()
       .expect("Unable to resolve domain")
       .collect()
}

fn display_ping_info(target: SocketAddr, elapsed_time: Duration) {
    let console_str = format!("{:?}: time={:>10} ms",
                                target,
                                format!("{:.5}", elapsed_time.as_secs_f64() * 1000.0));
    println!("{}", console_str.green());
}

fn display_statistic(total_time: Duration, count: u64, lose_count: u64) {
    println!("{}", "----- statistic -----".bold());
    println!("total time: {:?}", total_time);
    println!("Connect time: {}, recv time: {}, lose time: {} ({}%)",
            count,
            count - lose_count,
            lose_count,
            if lose_count == 0 { 0 } else { lose_count * 100 / count });
}

fn main() -> Result<()> {
    // load cli config
    let yaml = load_yaml!("cli.yaml");
    let args = App::from(yaml).get_matches();

    // parse args
    let target = args.value_of("Domain").unwrap();
    let count = args.value_of("Count")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap();

    // DNS resolve
    let server = resolve(target);
    println!("DNS lookup: {:?}", server);

    // ping
    let mut total_time = Duration::new(0, 0);
    let lose_count: u64 = 0;
    for _ in 0..count {
        let start_time = Instant::now();
        let mut stream = TcpStream::connect(target)
                                    .expect("Couldn't connect to the server...");
        let mut buffer = [0; 1024];

        stream.write(&[1]).expect("Couldn't send data to server...");
        stream.read(&mut buffer).expect("Couldn't recv data from server...");

        let elapsed_time = start_time.elapsed();

        display_ping_info(
            stream.peer_addr().unwrap(),
            elapsed_time);

        total_time += elapsed_time;
    }

    // statistic
    display_statistic(total_time, count ,lose_count);
    Ok(())
}
