use std::io::prelude::*;
use std::io::Result;
use std::net::TcpStream;
use std::error::Error;

fn main() -> Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:8000")
                                .expect("Couldn't connect to the server..");
    let mut buffer = [0; 1024];

    stream.write(&[1]).expect("Couldn't send data to server...");
    stream.read(&mut buffer).expect("Couldn't recv data from server...");
    println!("{:?}", buffer);
    Ok(())
}
