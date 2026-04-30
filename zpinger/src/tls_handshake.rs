//! TLS handshake pinger — opens a TCP connection and runs a TLS
//! handshake (ClientHello → ServerHello / Certificate / Finished),
//! then closes. Reports success when the rustls handshake completes;
//! certificate validation errors propagate as protocol errors.
//!
//! Reuses the shared rustls + webpki-roots stack from `crate::tls` —
//! same default trust store as the other TLS-aware pingers
//! (`HttpPinger`, `WebSocketPinger`, etc.).

use std::io::{self, Result};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::pinger::Pinger;
use crate::tls::default_client_config;
use crate::uri::get_uri;
use crate::util::with_timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_PORT: u16 = 443;

/// TLS handshake pinger — measures the time to complete a TLS
/// handshake against `target`. The handshake covers TCP connect +
/// ClientHello + ServerHello + Certificate + (key exchange) +
/// Finished; we don't send any application data on top, just bring
/// the session to the point where it's ready and close.
///
/// Accepts a `host:port`, a `host` (default port 443), or an
/// `https://host[:port]/...` URL — the URI parser is the same one
/// `HttpPinger` uses, so any of those work.
pub struct TlsPinger {
    pub target: String,
    pub timeout: Duration,
    tls_config: Option<Arc<ClientConfig>>,
}

impl TlsPinger {
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
}

#[async_trait]
impl Pinger for TlsPinger {
    async fn ping(&self) -> Result<()> {
        let uri = get_uri(&self.target);
        if uri.domain.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TLS target is missing a host",
            ));
        }
        let port = if uri.port > 0 {
            uri.port as u16
        } else {
            DEFAULT_PORT
        };
        let endpoint = format!("{}:{port}", uri.domain);
        let server_name = ServerName::try_from(uri.domain.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let config = self
            .tls_config
            .clone()
            .unwrap_or_else(default_client_config);

        with_timeout(self.timeout, async move {
            let tcp = TcpStream::connect(&endpoint).await?;
            let connector = TlsConnector::from(config);
            // Driving `connect` to completion brings us through
            // ClientHello → ServerHello → Certificate → Finished.
            // rustls validates the cert chain against the configured
            // trust anchors as part of this future; bad chains turn
            // into io::Error here.
            let _stream = connector.connect(server_name, tcp).await?;
            Ok(())
        })
        .await
    }
}
