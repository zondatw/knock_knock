use std::io::{self, Read, Result, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use tungstenite::client::client;
use tungstenite::protocol::Message;

use crate::pinger::Pinger;
use crate::tls::default_client_config;
use crate::uri::{get_uri, URI};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// WebSocket pinger — runs the full RFC 6455 client flow:
///   1. open TCP (and TLS for `wss://`),
///   2. complete the HTTP/1.1 Upgrade handshake,
///   3. send a control PING frame,
///   4. wait for the matching PONG frame,
///   5. close gracefully.
///
/// Step 1-2 measure connection setup; step 3-4 measure the actual
/// frame-layer round trip. Both are included in the timing the
/// `Pinger` trait reports.
pub struct WebSocketPinger {
    pub target: String,
    pub timeout: Duration,
    /// TLS client config used for `wss://` targets. `None` triggers
    /// the lazily-built default backed by `webpki-roots`.
    tls_config: Option<Arc<ClientConfig>>,
}

impl WebSocketPinger {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
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
}

impl Pinger for WebSocketPinger {
    fn ping(&self) -> Result<()> {
        let uri = get_uri(&self.target);
        let scheme = uri.scheme.to_ascii_lowercase();
        match scheme.as_str() {
            "ws" => self.ping_plain(&uri),
            "wss" => self.ping_tls(&uri),
            other => Err(io::Error::other(format!(
                "scheme '{other}' is not supported by WebSocketPinger (use ws:// or wss://)"
            ))),
        }
    }
}

impl WebSocketPinger {
    fn ping_plain(&self, uri: &URI) -> Result<()> {
        let endpoint = endpoint_for(uri, 80)?;
        let tcp = TcpStream::connect(&endpoint)?;
        tcp.set_read_timeout(Some(self.timeout))?;
        tcp.set_write_timeout(Some(self.timeout))?;
        run_handshake_and_ping(&self.target, tcp)
    }

    fn ping_tls(&self, uri: &URI) -> Result<()> {
        let endpoint = endpoint_for(uri, 443)?;
        let server_name = ServerName::try_from(uri.domain.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        let config = self
            .tls_config
            .clone()
            .unwrap_or_else(default_client_config);

        let conn = ClientConnection::new(config, server_name).map_err(io::Error::other)?;
        let tcp = TcpStream::connect(&endpoint)?;
        tcp.set_read_timeout(Some(self.timeout))?;
        tcp.set_write_timeout(Some(self.timeout))?;
        let stream = StreamOwned::new(conn, tcp);
        run_handshake_and_ping(&self.target, stream)
    }
}

fn run_handshake_and_ping<S: Read + Write>(target: &str, stream: S) -> Result<()> {
    let (mut ws, _response) = client(target, stream).map_err(handshake_err)?;

    ws.send(Message::Ping(Vec::new()))
        .map_err(tungstenite_err)?;

    loop {
        match ws.read().map_err(tungstenite_err)? {
            Message::Pong(_) => break,
            // The server may emit other frames before the pong (text /
            // binary on a chatty endpoint). Skip them.
            Message::Text(_) | Message::Binary(_) | Message::Frame(_) => continue,
            Message::Ping(payload) => {
                // Server-initiated ping — answer it and keep waiting.
                ws.send(Message::Pong(payload)).map_err(tungstenite_err)?;
            }
            Message::Close(frame) => {
                return Err(io::Error::other(format!(
                    "server closed before pong: {frame:?}"
                )));
            }
        }
    }

    let _ = ws.close(None);
    Ok(())
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

fn tungstenite_err(err: tungstenite::Error) -> io::Error {
    match err {
        tungstenite::Error::Io(e) => e,
        other => io::Error::other(other.to_string()),
    }
}

fn handshake_err<S: Read + Write>(
    err: tungstenite::HandshakeError<tungstenite::ClientHandshake<S>>,
) -> io::Error {
    match err {
        tungstenite::HandshakeError::Failure(e) => tungstenite_err(e),
        tungstenite::HandshakeError::Interrupted(_) => io::Error::other("handshake interrupted"),
    }
}
