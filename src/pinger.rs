use std::collections::HashMap;
use std::io::prelude::*;
use std::io::Result;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

#[path = "tests/test_pinger.rs"]
#[cfg(test)]
mod test_pinger;

fn get_domain_path(url: &str) -> &str {
    let s: Vec<&str> = url.split("/").collect();
    s[0]
}

pub fn resolve(url: &str) -> Vec<SocketAddr> {
    get_domain_path(url)
        .to_socket_addrs()
        .expect("Unable to resolve domain")
        .collect()
}

type Pinger = fn(&str) -> Result<()>;

pub struct PingHandler {
    pub protocol_map: HashMap<String, Pinger>,
}

impl PingHandler {
    pub fn add_pinger(&mut self, protocol: String, func: Pinger) {
        self.protocol_map.insert(protocol, func);
    }

    pub fn ping(&mut self, protocol: &str, target: &str) -> Result<Duration> {
        let start_time = Instant::now();

        match self.protocol_map[protocol](target) {
            Ok(_) => (),
            Err(err) => return Err(err),
        };

        let elapsed_time = start_time.elapsed();
        Ok(elapsed_time)
    }
}

pub fn tcping(target: &str) -> Result<()> {
    let mut stream = TcpStream::connect(target)?;
    let mut buffer = [0; 1024];

    //set timeout
    stream.set_read_timeout(Some(Duration::new(5, 0)))?;
    stream.set_write_timeout(Some(Duration::new(5, 0)))?;

    stream.write(&[1])?;
    stream.read(&mut buffer)?;
    Ok(())
}

pub fn udping(target: &str) -> Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let mut buffer = [0; 1024];
    socket.connect(target)?;

    //set timeout
    socket.set_read_timeout(Some(Duration::new(5, 0)))?;
    socket.set_write_timeout(Some(Duration::new(5, 0)))?;

    socket.send(&[1])?;
    socket.recv_from(&mut buffer)?;
    Ok(())
}

pub fn httping(target: &str) -> Result<()> {
    let mut stream = TcpStream::connect(get_domain_path(target))?;
    let mut buffer = [0; 1024];

    //set timeout
    stream.set_read_timeout(Some(Duration::new(5, 0)))?;
    stream.set_write_timeout(Some(Duration::new(5, 0)))?;

    stream.write(format!("GET {} HTTP/1.1\r\nConnection: close\r\n\r\n", target).as_bytes())?;
    stream.read(&mut buffer)?;
    Ok(())
}
