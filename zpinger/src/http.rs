use std::io::prelude::*;
use std::io::{self, Result};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};

use crate::pinger::Pinger;
use crate::tls::default_client_config;
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

/// HTTP / HTTPS pinger — opens a TCP connection (optionally wrapped in
/// TLS for `https://`), writes a single HTTP/1.1 request, reads the
/// response, and reports success based on the status line.
pub struct HttpPinger {
    pub method: HttpMethod,
    pub target: String,
    pub timeout: Duration,
    /// TLS client config used for `https://` targets. `None` triggers
    /// the lazily-built default backed by `webpki-roots`. Tests inject
    /// a custom config that trusts a self-signed cert.
    tls_config: Option<Arc<ClientConfig>>,
}

impl HttpPinger {
    pub fn new(method: HttpMethod, target: impl Into<String>) -> Self {
        Self {
            method,
            target: target.into(),
            timeout: DEFAULT_TIMEOUT,
            tls_config: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_tls_config(mut self, config: Arc<ClientConfig>) -> Self {
        self.tls_config = Some(config);
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

    fn ping_plain(&self, uri: &URI) -> Result<()> {
        let mut stream = TcpStream::connect(uri.host.as_str())?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        run_exchange(&mut stream, &self.build_request(uri))
    }

    fn ping_tls(&self, uri: &URI) -> Result<()> {
        let server_name = ServerName::try_from(uri.domain.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        let config = self
            .tls_config
            .clone()
            .unwrap_or_else(default_client_config);

        let conn = ClientConnection::new(config, server_name).map_err(io::Error::other)?;
        let tcp = TcpStream::connect(uri.host.as_str())?;
        tcp.set_read_timeout(Some(self.timeout))?;
        tcp.set_write_timeout(Some(self.timeout))?;
        let mut stream = StreamOwned::new(conn, tcp);
        run_exchange(&mut stream, &self.build_request(uri))
    }
}

impl Pinger for HttpPinger {
    fn ping(&self) -> Result<()> {
        let uri = get_uri(&self.target);
        let scheme = uri.scheme.to_ascii_lowercase();
        match scheme.as_str() {
            "" | "http" => self.ping_plain(&uri),
            "https" => self.ping_tls(&uri),
            other => Err(io::Error::other(format!(
                "scheme '{other}' is not supported by HttpPinger (use http:// or https://)"
            ))),
        }
    }
}

/// Send a request and validate the response status line. Generic over
/// the stream type so the same code drives both the plain TCP and the
/// rustls-wrapped paths.
fn run_exchange<S: Read + Write>(stream: &mut S, request: &str) -> Result<()> {
    stream.write_all(request.as_bytes())?;

    let mut buffer = [0u8; BUF_SIZE];
    let _ = stream.read(&mut buffer)?;

    let buffer_str = String::from_utf8_lossy(&buffer);
    let status_line = buffer_str.split("\r\n").next().unwrap_or("");

    // Reject anything that does not look like an HTTP response —
    // catches TLS alerts when someone speaks plain HTTP at a TLS port,
    // zero reads, and any other garbage.
    if !status_line.starts_with("HTTP/") {
        return Err(io::Error::other(
            "response is not HTTP/1.x (wrong port? wrong protocol?)",
        ));
    }

    for code in HTTP_UNCONNECT_STATUS_CODE {
        if status_line.contains(code) {
            return Err(io::Error::new(io::ErrorKind::NotFound, *code));
        }
    }
    Ok(())
}
