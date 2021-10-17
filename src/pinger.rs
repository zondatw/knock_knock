use std::io::prelude::*;
use std::io::Result;
use std::net::{TcpStream, UdpSocket};
use std::time::{Duration, Instant};
use std::collections::HashMap;

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

    stream.write(&[1])?;
    stream.read(&mut buffer)?;
    Ok(())
}

pub fn udping(target: &str) -> Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.connect(target)?;
    socket.set_read_timeout(Some(Duration::new(5, 0)))?;
    socket.set_write_timeout(Some(Duration::new(5, 0)))?;
    let mut buffer = [0; 1024];

    socket.send(&[1])?;
    socket.recv_from(&mut buffer)?;
    Ok(())
}



