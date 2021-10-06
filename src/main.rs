use std::io::prelude::*;
use std::io::Result;
use std::net::TcpStream;

fn main() -> Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:8000")?;
    let mut buffer = [0; 1024];

    stream.write(&[1])?;
    stream.read(&mut buffer)?;
    println!("{:?}", buffer);
    Ok(())
}
