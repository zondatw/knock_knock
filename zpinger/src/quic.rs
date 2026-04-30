//! QUIC pinger — measures the time to complete an RFC 9000 QUIC v1
//! handshake against `endpoint`. Reports success when the handshake
//! finishes (TLS 1.3 + transport parameters + ALPN agreement); we
//! don't open any HTTP/3 streams on top — that's intentional. The
//! point is to isolate the connection-establishment cost the way
//! `TlsPinger` does for TCP+TLS, but for the QUIC stack.
//!
//! Built on [`quinn`](https://crates.io/crates/quinn) with the
//! `runtime-tokio` + `rustls-ring` features so QUIC's own TLS layer
//! shares the ring crypto provider with the rest of the workspace.
//! Default ALPN is `h3` (HTTP/3) since most production QUIC servers
//! that you'd actually want to monitor speak HTTP/3; override via
//! `with_alpn` for `hq-29` / custom protocols.
//!
//! Schemes accepted: `quic://`, `https://` (treated as h3), or just
//! `host:port`. Default port 443.

use std::io::{self, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig as QuinnClientConfig, Endpoint};
use rustls::{ClientConfig, RootCertStore};

use crate::pinger::Pinger;
use crate::uri::get_uri;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_PORT: u16 = 443;
const DEFAULT_ALPN: &[u8] = b"h3";

/// QUIC pinger. Reports the time taken to complete the QUIC handshake
/// (UDP + Initial / Handshake / 1-RTT keys ready). Doesn't open
/// streams or speak HTTP/3 frames — handshake completion IS the
/// success signal.
pub struct QuicPinger {
    pub endpoint: String,
    pub timeout: Duration,
    pub alpn: Vec<Vec<u8>>,
    tls_config: Option<Arc<ClientConfig>>,
}

impl QuicPinger {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            timeout: DEFAULT_TIMEOUT,
            alpn: vec![DEFAULT_ALPN.to_vec()],
            tls_config: None,
        }
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    /// Replace the ALPN list. Default is `[b"h3".to_vec()]`. Pass an
    /// empty vec to send a TLS ClientHello with no ALPN extension at
    /// all — most servers will refuse, but a few legacy QUIC stacks
    /// expect that.
    pub fn with_alpn(mut self, alpn: Vec<Vec<u8>>) -> Self {
        self.alpn = alpn;
        self
    }

    /// Inject a custom rustls `ClientConfig`. The pinger will set the
    /// ALPN list on a clone before handing it to quinn, so caller-
    /// provided ALPN is overwritten by `self.alpn`. Use this for
    /// self-signed test endpoints (set the trust anchor here).
    pub fn with_tls_config(mut self, config: Arc<ClientConfig>) -> Self {
        self.tls_config = Some(config);
        self
    }
}

#[async_trait]
impl Pinger for QuicPinger {
    async fn ping(&self) -> Result<()> {
        let (host, port) = parse_endpoint(&self.endpoint)?;
        let server_addr = resolve_first(&host, port).await?;
        let crypto = build_rustls_config(self.tls_config.as_deref(), &self.alpn)?;
        let quic_crypto = QuicClientConfig::try_from(crypto)
            .map_err(|e| io::Error::other(format!("quinn rustls config: {e}")))?;
        let client_config = QuinnClientConfig::new(Arc::new(quic_crypto));

        let mut endpoint = Endpoint::client(unspecified_for(server_addr))
            .map_err(|e| io::Error::other(format!("quinn Endpoint::client: {e}")))?;
        endpoint.set_default_client_config(client_config);

        let connecting = endpoint
            .connect(server_addr, &host)
            .map_err(|e| io::Error::other(format!("quinn connect: {e}")))?;

        // Bound the handshake on our `timeout`. quinn's internal
        // `idle_timeout` is separate and would only fire after a stall;
        // for ping latency we want a single hard cap.
        match tokio::time::timeout(self.timeout, connecting).await {
            Ok(Ok(connection)) => {
                connection.close(0u32.into(), b"ping done");
                // Give quinn a beat to send the CONNECTION_CLOSE frame
                // before we drop the endpoint and the UDP socket
                // disappears. wait_idle returns immediately once all
                // pending datagrams have been flushed.
                endpoint.wait_idle().await;
                Ok(())
            }
            Ok(Err(e)) => Err(io::Error::other(format!("quinn handshake: {e}"))),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "QUIC handshake timed out",
            )),
        }
    }
}

/// Parse `quic://host[:port]/...`, `https://host[:port]/...`, or
/// `host[:port]` into `(host, port)`.
fn parse_endpoint(endpoint: &str) -> Result<(String, u16)> {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "QUIC endpoint is empty",
        ));
    }
    let uri = get_uri(trimmed);
    let scheme = uri.scheme.to_ascii_lowercase();
    match scheme.as_str() {
        "" | "quic" | "https" | "h3" => {}
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "scheme '{other}' is not supported by QuicPinger \
                     (use quic://, https://, or host:port)"
                ),
            ));
        }
    }
    if uri.domain.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "QUIC endpoint is missing a host",
        ));
    }
    let port = if uri.port > 0 {
        uri.port as u16
    } else {
        DEFAULT_PORT
    };
    Ok((uri.domain, port))
}

/// Resolve `host:port` and pick a single `SocketAddr` to dial. We
/// prefer IPv4 when both are returned because most local-loopback /
/// container test fixtures bind v4-only, and falling back from IPv6
/// to v4 means a 5-second timeout instead of a fast hit. On a
/// production internet host with both AAAA and A records this still
/// works — quinn doesn't care which family we pick.
async fn resolve_first(host: &str, port: u16) -> Result<SocketAddr> {
    let target = format!("{host}:{port}");
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(&target).await?.collect();
    if let Some(v4) = addrs.iter().find(|a| a.is_ipv4()) {
        return Ok(*v4);
    }
    addrs
        .into_iter()
        .next()
        .ok_or_else(|| io::Error::other(format!("DNS lookup returned no addresses for {target}")))
}

/// Pick the right wildcard local-bind address for the resolved
/// remote: `0.0.0.0:0` for IPv4, `[::]:0` for IPv6. quinn's UDP
/// socket has to be in the same family as the remote it's about to
/// dial.
fn unspecified_for(remote: SocketAddr) -> SocketAddr {
    if remote.is_ipv6() {
        "[::]:0".parse().unwrap()
    } else {
        "0.0.0.0:0".parse().unwrap()
    }
}

/// Build a rustls `ClientConfig` suitable for quinn: TLS 1.3 only, ring
/// crypto provider, ALPN list applied. If `inject` is supplied it's
/// used as the base (so a caller-provided trust anchor / cert verifier
/// flows through); otherwise we build a fresh config that trusts the
/// Mozilla webpki roots.
fn build_rustls_config(inject: Option<&ClientConfig>, alpn: &[Vec<u8>]) -> Result<ClientConfig> {
    let mut config = match inject {
        Some(c) => c.clone(),
        None => default_quic_config()?,
    };
    config.alpn_protocols = alpn.to_vec();
    Ok(config)
}

fn default_quic_config() -> Result<ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| io::Error::other(format!("rustls protocol: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_endpoint_handles_schemes() {
        assert_eq!(
            parse_endpoint("quic://example.com:8443").unwrap(),
            ("example.com".to_string(), 8443)
        );
        assert_eq!(
            parse_endpoint("https://example.com").unwrap(),
            ("example.com".to_string(), 443)
        );
        assert_eq!(
            parse_endpoint("example.com:443").unwrap(),
            ("example.com".to_string(), 443)
        );
        assert_eq!(
            parse_endpoint("example.com").unwrap(),
            ("example.com".to_string(), 443)
        );
    }

    #[test]
    fn parse_endpoint_rejects_unsupported_scheme() {
        let err = parse_endpoint("ws://example.com").unwrap_err();
        assert!(err.to_string().contains("scheme 'ws'"));
    }

    #[test]
    fn parse_endpoint_rejects_empty() {
        assert!(parse_endpoint("").is_err());
        assert!(parse_endpoint("   ").is_err());
    }

    #[test]
    fn unspecified_for_picks_v4_or_v6() {
        let v4: SocketAddr = "1.2.3.4:443".parse().unwrap();
        assert_eq!(unspecified_for(v4), "0.0.0.0:0".parse().unwrap());
        let v6: SocketAddr = "[2001:db8::1]:443".parse().unwrap();
        assert_eq!(unspecified_for(v6), "[::]:0".parse().unwrap());
    }
}
