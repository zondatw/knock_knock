use std::collections::HashMap;
use std::io::prelude::*;
use std::io::Result;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};
use regex::Regex;

#[path = "tests/test_pinger.rs"]
#[cfg(test)]
mod test_pinger;

const BUF_SIZE: usize = 0xFF;
const HTTP_UNCONNECT_STATUS_CODE: &'static [&'static str] = &["404", "501"];

pub struct URI {
    scheme: String,
    username: String,
    password: String,
    host: String,
    domain: String,
    port: i32,
    path: String,
    query: String,
    fragment: String,
}

impl URI {
    fn default() -> URI {
        URI {
            scheme: String::from(""),
            username: String::from(""),
            password: String::from(""),
            host: String::from(""),
            domain: String::from(""),
            port: 0,
            path: String::from("/"),
            query: String::from(""),
            fragment: String::from(""),
        }
    }

    fn parse(&mut self, url: &str) -> Result<()> {
        let rg_w_named = Regex::new(r"^((?P<scheme>[^:/?#]+)://)?((?P<username>\w+):(?P<password>\w+)@)?(?P<host>(?P<domain>[^/?#]*)(:(?P<port>\d*)))?(?P<path>[^?#]*)(\?(?P<query>[^#]*))?(#(?P<fragment>.*))?").unwrap(); match rg_w_named.captures(url) {
            Some(uri_parser) => {
                match uri_parser.name("scheme") {
                    Some(v) => self.scheme = v.as_str().to_string(),
                    None => self.scheme = String::from(""),
                }
                match uri_parser.name("host") {
                    Some(v) => self.host = v.as_str().to_string(),
                    None => self.host = String::from(""),
                }
                match uri_parser.name("username") {
                    Some(v) => self.username = v.as_str().to_string(),
                    None => self.username = String::from(""),
                }
                match uri_parser.name("password") {
                    Some(v) => self.password = v.as_str().to_string(),
                    None => self.password = String::from(""),
                }
                match uri_parser.name("domain") {
                    Some(v) => self.domain = v.as_str().to_string(),
                    None => self.domain = String::from(""),
                }
                match uri_parser.name("port") {
                    Some(v) => self.port = v.as_str().to_string().parse::<i32>().unwrap(),
                    None => self.port = 0,
                }
                match uri_parser.name("path") {
                    Some(v) => self.path = v.as_str().to_string(),
                    None => self.path = String::from("/"),
                }
                match uri_parser.name("query") {
                    Some(v) => self.query = v.as_str().to_string(),
                    None => self.query = String::from(""),
                }
                match uri_parser.name("fragment") {
                    Some(v) => self.fragment = v.as_str().to_string(),
                    None => self.fragment = String::from(""),
                }
            },
            None => return Result::Err(Error::new(ErrorKind::InvalidData, "Invalid url")),
        }
        Ok(())
    }

    fn get_url(&mut self) -> String {
        format!(
            "{}://{}:{}@{}{}{}#{}",
            self.scheme,
            self.username,
            self.password,
            self.host,
            self.path,
            self.query,
            self.fragment,
        )
    }

    fn display(&mut self) {
        println!("URI:\n\
             \tScheme: {:?}\n\
             \tUsername: {:?}\n\
             \tPassword: {:?}\n\
             \tHost: {:?}\n\
             \tDomain: {:?}\n\
             \tPort: {:?}\n\
             \tPath: {:?}\n\
             \tQuery: {:?}\n\
             \tFragment: {:?}\n",
             self.scheme,
             self.username,
             self.password,
             self.host,
             self.domain,
             self.port,
             self.path,
             self.query,
             self.fragment,
            );
    }
}

pub fn get_uri(url: &str) -> URI {
    let mut uri = URI::default();
    uri.parse(url);
    uri
}

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

fn httping(target: &str, body: String) -> Result<()> {
    let mut stream = TcpStream::connect(get_domain_path(target))?;
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
    let mut uri = get_uri(target);
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
    let mut uri = get_uri(target);
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
    let mut uri = get_uri(target);
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
    let mut uri = get_uri(target);
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
    let mut uri = get_uri(target);
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
    let mut uri = get_uri(target);
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


