use std::io::{Read, Result, Write};
use std::net::{SocketAddr, TcpListener, ToSocketAddrs, UdpSocket};
use std::thread;

const BUF_SIZE: usize = 1024;

pub fn start_tcp_echo<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut s = stream;
                let mut buf = [0u8; BUF_SIZE];
                if let Ok(n) = s.read(&mut buf) {
                    let _ = s.write_all(&buf[..n]);
                }
            });
        }
    });
    Ok(bound)
}

pub fn start_udp_echo<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let socket = UdpSocket::bind(addr)?;
    let bound = socket.local_addr()?;
    thread::spawn(move || {
        let mut buf = [0u8; BUF_SIZE];
        loop {
            if let Ok((n, src)) = socket.recv_from(&mut buf) {
                let _ = socket.send_to(&buf[..n], src);
            }
        }
    });
    Ok(bound)
}

pub fn start_http_ok<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut s = stream;
                let mut buf = [0u8; BUF_SIZE];
                let _ = s.read(&mut buf);
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            });
        }
    });
    Ok(bound)
}
