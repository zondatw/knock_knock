use std::io::{self, Result};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::level4::with_timeout;
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

    fn build_request(&self, uri: &URI, host_header: &str) -> String {
        let method = self.method.as_str();
        let path = if uri.path.is_empty() { "/" } else { &uri.path };
        if self.method.has_body() {
            format!(
                "{method} {path} HTTP/1.1\r\n\
                 Host: {host_header}\r\n\
                 User-Agent: Knock Knock\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: 2\r\n\
                 \r\n\
                 {{}}\r\n\
                 \r\n",
            )
        } else {
            format!(
                "{method} {path} HTTP/1.1\r\n\
                 Host: {host_header}\r\n\
                 User-Agent: Knock Knock\r\n\
                 \r\n",
            )
        }
    }

    async fn ping_plain(&self, uri: &URI) -> Result<()> {
        let endpoint = endpoint_for(uri, 80)?;
        let request = self.build_request(uri, &endpoint);
        with_timeout(self.timeout, async move {
            let mut stream = TcpStream::connect(&endpoint).await?;
            run_exchange(&mut stream, &request).await
        })
        .await
    }

    async fn ping_tls(&self, uri: &URI) -> Result<()> {
        let endpoint = endpoint_for(uri, 443)?;
        let server_name = ServerName::try_from(uri.domain.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let config = self
            .tls_config
            .clone()
            .unwrap_or_else(default_client_config);
        let request = self.build_request(uri, &endpoint);

        with_timeout(self.timeout, async move {
            let tcp = TcpStream::connect(&endpoint).await?;
            let connector = TlsConnector::from(config);
            let mut stream = connector.connect(server_name, tcp).await?;
            run_exchange(&mut stream, &request).await
        })
        .await
    }
}

#[async_trait]
impl Pinger for HttpPinger {
    async fn ping(&self) -> Result<()> {
        let uri = get_uri(&self.target);
        let scheme = uri.scheme.to_ascii_lowercase();
        match scheme.as_str() {
            "" | "http" => self.ping_plain(&uri).await,
            "https" => self.ping_tls(&uri).await,
            other => Err(io::Error::other(format!(
                "scheme '{other}' is not supported by HttpPinger (use http:// or https://)"
            ))),
        }
    }
}

fn endpoint_for(uri: &URI, default_port: u16) -> Result<String> {
    if uri.domain.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing host in target URL",
        ));
    }
    let port = if uri.port > 0 {
        uri.port as u16
    } else {
        default_port
    };
    Ok(format!("{}:{}", uri.domain, port))
}

/// Send a request and validate the response status line. Generic over
/// the stream type so the same code drives both the plain TCP and the
/// rustls-wrapped paths.
async fn run_exchange<S>(stream: &mut S, request: &str) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream.write_all(request.as_bytes()).await?;

    let mut buffer = [0u8; BUF_SIZE];
    let _ = stream.read(&mut buffer).await?;

    let buffer_str = String::from_utf8_lossy(&buffer);
    let status_line = buffer_str.split("\r\n").next().unwrap_or("");

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
