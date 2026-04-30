//! RTSP pinger — sends an `OPTIONS` request (RFC 2326 §10.1) over TCP
//! and validates the `RTSP/1.0 200` response. RTSP's `OPTIONS` method
//! is the spec-mandated keepalive: every conformant server must accept
//! it and answer with the list of supported methods, no media-session
//! state required. Hand-rolled wire format — RTSP looks like HTTP but
//! isn't HTTP, so reusing HttpPinger isn't worth the indirection.
//!
//! Plain `rtsp://` runs over TCP/554; `rtsps://` (RFC 7826 §19) runs
//! over TLS, default port 322 — same rustls + webpki-roots stack as
//! the other TLS-aware pingers.
//!
//! Wire shape:
//! ```text
//! OPTIONS rtsp://host:port/ RTSP/1.0\r\n
//! CSeq: 1\r\n
//! User-Agent: zpinger/0.6\r\n
//! \r\n
//! ```
//! Response we accept: starts with `RTSP/1.0 200`. Anything else
//! (including 4xx / 5xx) is treated as failure — a misbehaving server
//! returning 4xx to OPTIONS isn't useful for liveness.
//!
//! Validation stops at the first `\r\n\r\n` or `MAX_RESPONSE_BYTES`
//! whichever comes first; we don't care about the body, only the
//! status line.
//!
//! [RFC 2326 §10.1]: https://www.rfc-editor.org/rfc/rfc2326#section-10.1

use std::io::{self, Result};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::pinger::Pinger;
use crate::tls::default_client_config;
use crate::uri::{get_uri, URI};
use crate::util::with_timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_PORT_PLAIN: u16 = 554;
const DEFAULT_PORT_TLS: u16 = 322;

const MAX_RESPONSE_BYTES: usize = 4096;
const HEADER_TERMINATOR: &[u8] = b"\r\n\r\n";

const STATUS_LINE_PREFIX: &[u8] = b"RTSP/1.0 200";

/// RTSP pinger.
///
/// Speaks plain TCP for `rtsp://` (or schemeless) targets and rustls
/// TLS for `rtsps://`. Reuses the webpki-roots default trust store
/// unless a caller-supplied `ClientConfig` is injected via
/// `with_tls_config`.
pub struct RtspPinger {
    pub target: String,
    pub timeout: Duration,
    tls_config: Option<Arc<ClientConfig>>,
}

impl RtspPinger {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            timeout: DEFAULT_TIMEOUT,
            tls_config: None,
        }
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    pub fn with_tls_config(mut self, config: Arc<ClientConfig>) -> Self {
        self.tls_config = Some(config);
        self
    }

    async fn ping_plain(&self, uri: &URI) -> Result<()> {
        let endpoint = endpoint_for(uri, DEFAULT_PORT_PLAIN)?;
        let request = build_options_request(&uri.domain, port_or(uri, DEFAULT_PORT_PLAIN));
        with_timeout(self.timeout, async move {
            let mut stream = TcpStream::connect(&endpoint).await?;
            stream.write_all(&request).await?;
            validate_response(&mut stream).await
        })
        .await
    }

    async fn ping_tls(&self, uri: &URI) -> Result<()> {
        let endpoint = endpoint_for(uri, DEFAULT_PORT_TLS)?;
        let server_name = ServerName::try_from(uri.domain.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let config = self
            .tls_config
            .clone()
            .unwrap_or_else(default_client_config);
        let request = build_options_request(&uri.domain, port_or(uri, DEFAULT_PORT_TLS));
        with_timeout(self.timeout, async move {
            let tcp = TcpStream::connect(&endpoint).await?;
            let connector = TlsConnector::from(config);
            let mut stream = connector.connect(server_name, tcp).await?;
            stream.write_all(&request).await?;
            validate_response(&mut stream).await
        })
        .await
    }
}

#[async_trait]
impl Pinger for RtspPinger {
    async fn ping(&self) -> Result<()> {
        let uri = get_uri(&self.target);
        if uri.domain.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "RTSP target is missing a host",
            ));
        }
        let scheme = uri.scheme.to_ascii_lowercase();
        match scheme.as_str() {
            "" | "rtsp" => self.ping_plain(&uri).await,
            "rtsps" => self.ping_tls(&uri).await,
            other => Err(io::Error::other(format!(
                "scheme '{other}' is not supported by RtspPinger (use rtsp:// or rtsps://)"
            ))),
        }
    }
}

fn port_or(uri: &URI, default: u16) -> u16 {
    if uri.port > 0 {
        uri.port as u16
    } else {
        default
    }
}

fn endpoint_for(uri: &URI, default_port: u16) -> Result<String> {
    Ok(format!("{}:{}", uri.domain, port_or(uri, default_port)))
}

/// Build the OPTIONS request line + headers. The Request-URI follows
/// RFC 2326 §6.1: `rtsp://host[:port]/`. The `*` form is also legal
/// per the spec but some real-world servers don't accept it, so we
/// stick with the explicit URL.
fn build_options_request(domain: &str, port: u16) -> Vec<u8> {
    let request_uri = format!("rtsp://{domain}:{port}/");
    let user_agent = concat!("zpinger/", env!("CARGO_PKG_VERSION"));
    format!(
        "OPTIONS {request_uri} RTSP/1.0\r\n\
         CSeq: 1\r\n\
         User-Agent: {user_agent}\r\n\
         \r\n"
    )
    .into_bytes()
}

/// Read until `\r\n\r\n` (end of headers) or the buffer is full, then
/// confirm the first line begins with `RTSP/1.0 200`.
async fn validate_response<S>(stream: &mut S) -> Result<()>
where
    S: AsyncRead + Unpin,
{
    let mut buf = Vec::with_capacity(512);
    let mut chunk = [0u8; 256];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if find_header_terminator(&buf).is_some() || buf.len() >= MAX_RESPONSE_BYTES {
            break;
        }
    }
    if !buf.starts_with(STATUS_LINE_PREFIX) {
        let preview = String::from_utf8_lossy(&buf[..buf.len().min(64)]);
        return Err(io::Error::other(format!(
            "RTSP response did not start with `RTSP/1.0 200`: {preview:?}"
        )));
    }
    Ok(())
}

fn find_header_terminator(buf: &[u8]) -> Option<usize> {
    buf.windows(HEADER_TERMINATOR.len())
        .position(|w| w == HEADER_TERMINATOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_includes_explicit_url() {
        let req = build_options_request("example.com", 554);
        let s = std::str::from_utf8(&req).unwrap();
        assert!(s.starts_with("OPTIONS rtsp://example.com:554/ RTSP/1.0\r\n"));
        assert!(s.contains("CSeq: 1\r\n"));
        assert!(s.contains("User-Agent: zpinger/"));
        assert!(s.ends_with("\r\n\r\n"));
    }

    #[test]
    fn build_request_uses_explicit_port() {
        let req = build_options_request("host", 8554);
        assert!(std::str::from_utf8(&req)
            .unwrap()
            .starts_with("OPTIONS rtsp://host:8554/ RTSP/1.0\r\n"));
    }

    #[tokio::test]
    async fn validate_response_accepts_200_with_body() {
        let canned = b"RTSP/1.0 200 OK\r\nCSeq: 1\r\nPublic: OPTIONS, DESCRIBE\r\n\r\n";
        validate_response(&mut &canned[..]).await.unwrap();
    }

    #[tokio::test]
    async fn validate_response_rejects_4xx() {
        let canned = b"RTSP/1.0 404 Not Found\r\nCSeq: 1\r\n\r\n";
        let err = validate_response(&mut &canned[..]).await.unwrap_err();
        assert!(err.to_string().contains("RTSP/1.0 200"));
    }

    #[tokio::test]
    async fn validate_response_rejects_http() {
        // Real-world misconfiguration: port 80 server on RTSP host.
        let canned = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        assert!(validate_response(&mut &canned[..]).await.is_err());
    }

    #[tokio::test]
    async fn validate_response_rejects_eof() {
        let canned: &[u8] = b"";
        assert!(validate_response(&mut &canned[..]).await.is_err());
    }
}
