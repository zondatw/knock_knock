use std::io::prelude::*;
use std::io::Result;
use std::net::{TcpStream, ToSocketAddrs, SocketAddr};
use std::error::Error;

fn resolve(domain: &str) -> Vec<SocketAddr> {
   domain.to_socket_addrs()
       .expect("Unable to resolve domain")
       .collect()
}

fn main() -> Result<()> {
    let target = "google.com:80";

    let server = resolve(target);
    println!("{:?}", server);

    let mut stream = TcpStream::connect(target)
                                .expect("Couldn't connect to the server..");
    let mut buffer = [0; 1024];

    println!("{:?}", stream.peer_addr().unwrap());
    stream.write(&[1]).expect("Couldn't send data to server...");
    stream.read(&mut buffer).expect("Couldn't recv data from server...");
    println!("{:?}", buffer);
    Ok(())
}
