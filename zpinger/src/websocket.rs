use std::io::{self, Result};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_tungstenite::tungstenite::Message;

use crate::pinger::Pinger;
use crate::tls::default_client_config;
use crate::uri::{get_uri, URI};
use crate::util::with_timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// WebSocket pinger — runs the full RFC 6455 client flow:
///   1. open TCP (and TLS for `wss://`),
///   2. complete the HTTP/1.1 Upgrade handshake,
///   3. send a control PING frame,
///   4. wait for the matching PONG frame,
///   5. close gracefully.
pub struct WebSocketPinger {
    pub target: String,
    pub timeout: Duration,
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

    async fn ping_plain(&self, uri: &URI) -> Result<()> {
        let endpoint = endpoint_for(uri, 80)?;
        let target = self.target.clone();
        with_timeout(self.timeout, async move {
            let tcp = TcpStream::connect(&endpoint).await?;
            run_handshake_and_ping(&target, tcp).await
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
        let target = self.target.clone();
        with_timeout(self.timeout, async move {
            let tcp = TcpStream::connect(&endpoint).await?;
            let connector = TlsConnector::from(config);
            let stream = connector.connect(server_name, tcp).await?;
            run_handshake_and_ping(&target, stream).await
        })
        .await
    }
}

#[async_trait]
impl Pinger for WebSocketPinger {
    async fn ping(&self) -> Result<()> {
        let uri = get_uri(&self.target);
        let scheme = uri.scheme.to_ascii_lowercase();
        match scheme.as_str() {
            "ws" => self.ping_plain(&uri).await,
            "wss" => self.ping_tls(&uri).await,
            other => Err(io::Error::other(format!(
                "scheme '{other}' is not supported by WebSocketPinger (use ws:// or wss://)"
            ))),
        }
    }
}

async fn run_handshake_and_ping<S>(target: &str, stream: S) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut ws, _response) = tokio_tungstenite::client_async(target, stream)
        .await
        .map_err(tungstenite_err)?;

    ws.send(Message::Ping(Default::default()))
        .await
        .map_err(tungstenite_err)?;

    loop {
        match ws.next().await {
            Some(Ok(Message::Pong(_))) => break,
            // Server-initiated ping — answer it and keep waiting.
            Some(Ok(Message::Ping(payload))) => {
                ws.send(Message::Pong(payload))
                    .await
                    .map_err(tungstenite_err)?;
            }
            // Servers may emit text / binary frames before our pong.
            Some(Ok(Message::Text(_)))
            | Some(Ok(Message::Binary(_)))
            | Some(Ok(Message::Frame(_))) => continue,
            Some(Ok(Message::Close(frame))) => {
                return Err(io::Error::other(format!(
                    "server closed before pong: {frame:?}"
                )));
            }
            Some(Err(e)) => return Err(tungstenite_err(e)),
            None => {
                return Err(io::Error::other("stream ended before pong"));
            }
        }
    }

    let _ = ws.close(None).await;
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

fn tungstenite_err(err: tokio_tungstenite::tungstenite::Error) -> io::Error {
    use tokio_tungstenite::tungstenite::Error as E;
    match err {
        E::Io(e) => e,
        other => io::Error::other(other.to_string()),
    }
}
