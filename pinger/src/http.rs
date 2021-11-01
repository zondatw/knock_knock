use std::io::Result;
use std::io::prelude::*;
use std::io::{Error, ErrorKind};
use std::net::{TcpStream};
use std::time::{Duration};

use crate::uri::{get_uri};
use crate::{get_host_path};
use crate::{BUF_SIZE, HTTP_UNCONNECT_STATUS_CODE};

fn httping(target: &str, body: String) -> Result<()> {
    let mut stream = TcpStream::connect(get_host_path(target).as_str())?;
    let mut buffer = [0; BUF_SIZE];

    //set timeout
    stream.set_read_timeout(Some(Duration::new(5, 0)))?; stream.set_write_timeout(Some(Duration::new(5, 0)))?;

    stream.write(body.as_bytes())?;
    stream.read(&mut buffer)?;

    let buffer_str = String::from_utf8_lossy(&buffer);
    let header: Vec<&str> = buffer_str.split("\r\n").collect();
    for status_code in HTTP_UNCONNECT_STATUS_CODE {
        if header[0].contains(status_code) {
            return Result::Err(Error::new(ErrorKind::NotFound, "404"));
        }
    }
    Ok(())
}

pub fn httping_connect(target: &str) -> Result<()> {
    let uri = get_uri(target);
    httping(
        target,
        format!(
            "CONNECT {} HTTP/1.1\r\n\
            Host: {}\r\n\
            User-Agent: Knock Knock\r\n\
            \r\n",
            uri.path,
            uri.host,
        ),
    )?;
    Ok(())
}

pub fn httping_get(target: &str) -> Result<()> {
    let uri = get_uri(target);
    httping(
        target,
        format!(
            "GET {} HTTP/1.1\r\n\
            Host: {}\r\n\
            User-Agent: Knock Knock\r\n\
            Connection: close\r\n\
            \r\n",
            uri.path,
            uri.host,
        ),
    )?;
    Ok(())
}

pub fn httping_post(target: &str) -> Result<()> {
    let uri = get_uri(target);
    httping(
        target,
        format!(
            "POST {} HTTP/1.1\r\n\
            Host: {}\r\n\
            User-Agent: Knock Knock\r\n\
            Content-Type: application/json\r\n\
            Content-Length: 2\r\n\
            \r\n\
            {}\r\n\
            \r\n",
            uri.path,
            uri.host,
            "{}",
        ),
    )?;
    Ok(())
}

pub fn httping_put(target: &str) -> Result<()> {
    let uri = get_uri(target);
    httping(
        target,
        format!(
            "PUT {} HTTP/1.1\r\n\
            Host: {}\r\n\
            User-Agent: Knock Knock\r\n\
            Content-Type: application/json\r\n\
            Content-Length: 2\r\n\
            \r\n\
            {}\r\n\
            \r\n",
            uri.path,
            uri.host,
            "{}",
        ),
    )?;
    Ok(())
}

pub fn httping_delete(target: &str) -> Result<()> {
    let uri = get_uri(target);
    httping(
        target,
        format!(
            "DELETE {} HTTP/1.1\r\n\
            Host: {}\r\n\
            User-Agent: Knock Knock\r\n\
            \r\n",
            uri.path,
            uri.host,
        ),
    )?;
    Ok(())
}
pub fn httping_patch(target: &str) -> Result<()> {
    let uri = get_uri(target);
    httping(
        target,
        format!(
            "PATCH {} HTTP/1.1\r\n\
            Host: {}\r\n\
            User-Agent: Knock Knock\r\n\
            Content-Type: application/json\r\n\
            Content-Length: 2\r\n\
            \r\n\
            {}\r\n\
            \r\n",
            uri.path,
            uri.host,
            "{}",
        ),
    )?;
    Ok(())
}
