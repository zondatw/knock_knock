use std::io::prelude::*;
use std::io::Result;
use std::net::{TcpStream, UdpSocket};
use std::time::Duration;

use crate::BUF_SIZE;

pub fn tcping(target: &str) -> Result<()> {
    let mut stream = TcpStream::connect(target)?;
    let mut buffer = [0; BUF_SIZE];

    //set timeout
    stream.set_read_timeout(Some(Duration::new(5, 0)))?;
    stream.set_write_timeout(Some(Duration::new(5, 0)))?;

    stream.write(&[1])?;
    stream.read(&mut buffer)?;
    Ok(())
}

pub fn udping(target: &str) -> Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let mut buffer = [0; BUF_SIZE];
    socket.connect(target)?;

    //set timeout
    socket.set_read_timeout(Some(Duration::new(5, 0)))?;
    socket.set_write_timeout(Some(Duration::new(5, 0)))?;

    socket.send(&[1])?;
    socket.recv_from(&mut buffer)?;
    Ok(())
}
