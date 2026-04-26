use std::io::prelude::*;
use std::io::{self, Result};
use std::net::TcpStream;
use std::time::Duration;

use crate::pinger::Pinger;
use crate::uri::{get_uri, URI};
use crate::{BUF_SIZE, HTTP_UNCONNECT_STATUS_CODE};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Connect,
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl HttpMethod {
    fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Connect => "CONNECT",
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Patch => "PATCH",
        }
    }

    fn has_body(self) -> bool {
        matches!(self, HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch)
    }
}

/// HTTP pinger — opens a TCP connection to `target`, writes a single
/// HTTP/1.1 request, reads the response, and reports success based on
/// the status line.
///
/// Plain HTTP only. HTTPS / TLS is rejected up front in this PR; a
/// later PR will add a `rustls`-backed TLS layer.
pub struct HttpPinger {
    pub method: HttpMethod,
    pub target: String,
    pub timeout: Duration,
}

impl HttpPinger {
    pub fn new(method: HttpMethod, target: impl Into<String>) -> Self {
        Self {
            method,
            target: target.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn build_request(&self, uri: &URI) -> String {
        let method = self.method.as_str();
        if self.method.has_body() {
            format!(
                "{method} {} HTTP/1.1\r\n\
                 Host: {}\r\n\
                 User-Agent: Knock Knock\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: 2\r\n\
                 \r\n\
                 {{}}\r\n\
                 \r\n",
                uri.path, uri.host,
            )
        } else {
            format!(
                "{method} {} HTTP/1.1\r\n\
                 Host: {}\r\n\
                 User-Agent: Knock Knock\r\n\
                 \r\n",
                uri.path, uri.host,
            )
        }
    }
}

impl Pinger for HttpPinger {
    fn ping(&self) -> Result<()> {
        let uri = get_uri(&self.target);

        // Reject TLS schemes — plain TCP can't speak TLS, and the old
        // implementation silently returned Ok in this case (the status
        // code check only matched "404"/"501" substrings, which TLS
        // alert bytes never contain). Better to fail fast and loud.
        let scheme = uri.scheme.to_ascii_lowercase();
        if scheme == "https" || scheme == "wss" {
            return Err(io::Error::other(format!(
                "scheme '{scheme}' requires TLS; not yet supported in this build"
            )));
        }

        let mut stream = TcpStream::connect(uri.host.as_str())?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        let request = self.build_request(&uri);
        stream.write_all(request.as_bytes())?;

        let mut buffer = [0u8; BUF_SIZE];
        let _ = stream.read(&mut buffer)?;

        let buffer_str = String::from_utf8_lossy(&buffer);
        let status_line = buffer_str.split("\r\n").next().unwrap_or("");

        // Reject anything that does not look like an HTTP response.
        // Catches TLS alerts (when someone points us at port 443), zero
        // reads, and other garbage — the old code would have returned
        // Ok in all of these because the 404/501 substring check
        // happens to miss them.
        if !status_line.starts_with("HTTP/") {
            return Err(io::Error::other(
                "response is not HTTP/1.x (wrong port? TLS endpoint?)",
            ));
        }

        for code in HTTP_UNCONNECT_STATUS_CODE {
            if status_line.contains(code) {
                return Err(io::Error::new(io::ErrorKind::NotFound, *code));
            }
        }
        Ok(())
    }
}

// -- Backward-compat free functions ----------------------------------
//
// Kept so the existing PingHandler HashMap<String, fn(&str) -> Result>
// dispatch continues to work without changes. A later PR will drop
// these once the dispatcher migrates to trait objects.

pub fn httping_connect(target: &str) -> Result<()> {
    HttpPinger::new(HttpMethod::Connect, target).ping()
}

pub fn httping_get(target: &str) -> Result<()> {
    HttpPinger::new(HttpMethod::Get, target).ping()
}

pub fn httping_post(target: &str) -> Result<()> {
    HttpPinger::new(HttpMethod::Post, target).ping()
}

pub fn httping_put(target: &str) -> Result<()> {
    HttpPinger::new(HttpMethod::Put, target).ping()
}

pub fn httping_delete(target: &str) -> Result<()> {
    HttpPinger::new(HttpMethod::Delete, target).ping()
}

pub fn httping_patch(target: &str) -> Result<()> {
    HttpPinger::new(HttpMethod::Patch, target).ping()
}
