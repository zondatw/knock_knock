//! RTMP pinger — runs the simple Adobe RTMP §5.2.1 handshake and
//! reports success when it completes.
//!
//! Wire shape (3073 bytes each direction):
//! ```text
//! Client → Server:  C0 (1 byte = 0x03)         [RTMP version 3]
//!                   C1 (1536 bytes random)
//!                   C2 (1536 bytes = echo of S1)
//! Server → Client:  S0 (1 byte = 0x03)
//!                   S1 (1536 bytes)
//!                   S2 (1536 bytes = echo of C1)
//! ```
//! We send `C0 || C1`, read `S0 || S1 || S2`, send `C2 = S1`, then
//! close. After this exchange completes the handshake is over and the
//! server is ready to receive a `connect` AMF command — for our
//! liveness purposes, that point IS the success signal: it proves the
//! peer speaks RTMP version 3 and got past TCP / TLS plumbing.
//!
//! `rtmp://` runs over plain TCP (default port 1935); `rtmps://` runs
//! over TLS (default port 443) — same rustls + webpki-roots stack as
//! the other TLS-aware pingers.
//!
//! [Adobe RTMP §5.2.1]: https://rtmp.veriskope.com/pdf/rtmp_specification_1.0.pdf

use std::io::{self, Result};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::pinger::Pinger;
use crate::tls::default_client_config;
use crate::uri::{get_uri, URI};
use crate::util::with_timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_PORT_PLAIN: u16 = 1935;
const DEFAULT_PORT_TLS: u16 = 443;

const RTMP_VERSION: u8 = 3;
const HANDSHAKE_PAYLOAD_LEN: usize = 1536;

/// RTMP pinger.
///
/// Speaks plain TCP for `rtmp://` (or schemeless) targets and rustls
/// TLS for `rtmps://`. Reuses the webpki-roots default trust store
/// unless a caller-supplied `ClientConfig` is injected via
/// `with_tls_config`.
pub struct RtmpPinger {
    pub target: String,
    pub timeout: Duration,
    tls_config: Option<Arc<ClientConfig>>,
}

impl RtmpPinger {
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
        with_timeout(self.timeout, async move {
            let mut stream = TcpStream::connect(&endpoint).await?;
            run_handshake(&mut stream).await
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
        with_timeout(self.timeout, async move {
            let tcp = TcpStream::connect(&endpoint).await?;
            let connector = TlsConnector::from(config);
            let mut stream = connector.connect(server_name, tcp).await?;
            run_handshake(&mut stream).await
        })
        .await
    }
}

#[async_trait]
impl Pinger for RtmpPinger {
    async fn ping(&self) -> Result<()> {
        let uri = get_uri(&self.target);
        if uri.domain.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "RTMP target is missing a host",
            ));
        }
        let scheme = uri.scheme.to_ascii_lowercase();
        match scheme.as_str() {
            "" | "rtmp" => self.ping_plain(&uri).await,
            "rtmps" => self.ping_tls(&uri).await,
            other => Err(io::Error::other(format!(
                "scheme '{other}' is not supported by RtmpPinger (use rtmp:// or rtmps://)"
            ))),
        }
    }
}

fn endpoint_for(uri: &URI, default_port: u16) -> Result<String> {
    let port = if uri.port > 0 {
        uri.port as u16
    } else {
        default_port
    };
    Ok(format!("{}:{}", uri.domain, port))
}

/// Drive the simple RTMP handshake to completion over an already-
/// established stream (plain TCP or TLS).
async fn run_handshake<S>(stream: &mut S) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // Send C0 + C1 in one call so the server can immediately start
    // responding without a partial read.
    let c1 = build_c1();
    let mut c0_c1 = Vec::with_capacity(1 + HANDSHAKE_PAYLOAD_LEN);
    c0_c1.push(RTMP_VERSION);
    c0_c1.extend_from_slice(&c1);
    stream.write_all(&c0_c1).await?;

    // S0 — single version byte; spec requires 0x03 for RTMP version 3.
    let mut s0 = [0u8; 1];
    stream.read_exact(&mut s0).await?;
    if s0[0] != RTMP_VERSION {
        return Err(io::Error::other(format!(
            "RTMP S0 returned version {} (expected {RTMP_VERSION})",
            s0[0]
        )));
    }

    // S1 (1536 bytes). We need the bytes verbatim because C2 must echo
    // S1 back. The contents themselves are server-chosen and we don't
    // validate the time / random fields — the spec lets servers fill
    // these however they like.
    let mut s1 = [0u8; HANDSHAKE_PAYLOAD_LEN];
    stream.read_exact(&mut s1).await?;

    // S2 (1536 bytes). Should echo our C1, but in practice servers
    // diverge here (e.g., nginx-rtmp returns the time field with its
    // own write timestamp). We don't enforce equality — once we've
    // successfully read 1536 bytes, the handshake is functionally
    // complete from a liveness standpoint.
    let mut s2 = [0u8; HANDSHAKE_PAYLOAD_LEN];
    stream.read_exact(&mut s2).await?;

    // C2 = echo of S1.
    stream.write_all(&s1).await?;

    Ok(())
}

/// Build C1: 4-byte timestamp || 4-byte zero || 1528 bytes pseudo-random.
/// The "random" doesn't need to be cryptographically strong — many
/// real RTMP clients use a counter or zero-filled block. Time-derived
/// here for cheap uniqueness across concurrent pings.
fn build_c1() -> [u8; HANDSHAKE_PAYLOAD_LEN] {
    let mut buf = [0u8; HANDSHAKE_PAYLOAD_LEN];
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    // bytes 0..4: time (low 32 bits of millis derivation; arbitrary OK)
    buf[0..4].copy_from_slice(&nanos.to_be_bytes());
    // bytes 4..8: zero (spec says 0; some implementations stuff a
    // version vector here, but Flash-compatible servers accept zero).
    buf[4..8].copy_from_slice(&[0; 4]);
    // bytes 8..1536: cheap byte fill that also acts as the per-ping
    // entropy. Server's S2 must echo this back if it follows the spec
    // strictly, but we don't enforce that.
    let mut acc = nanos;
    for chunk in buf[8..].chunks_mut(4) {
        // xorshift32 seed; not crypto, just non-zero variation.
        acc ^= acc.wrapping_shl(13);
        acc ^= acc >> 17;
        acc ^= acc.wrapping_shl(5);
        let bytes = acc.to_be_bytes();
        for (i, b) in chunk.iter_mut().enumerate() {
            *b = bytes[i];
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[test]
    fn c1_is_1536_bytes() {
        let c1 = build_c1();
        assert_eq!(c1.len(), HANDSHAKE_PAYLOAD_LEN);
        // bytes 4..8 must be zero per spec.
        assert_eq!(&c1[4..8], &[0u8; 4]);
    }

    #[tokio::test]
    async fn handshake_round_trip() {
        // Wire up an in-memory bidirectional pipe and play a server
        // role on the far end.
        let (mut client, mut server) = duplex(8192);

        let server_task = tokio::spawn(async move {
            // Read C0 + C1.
            let mut c0 = [0u8; 1];
            server.read_exact(&mut c0).await.unwrap();
            assert_eq!(c0[0], RTMP_VERSION);
            let mut c1 = [0u8; HANDSHAKE_PAYLOAD_LEN];
            server.read_exact(&mut c1).await.unwrap();

            // Send S0 + S1 + S2 (S2 = echo of C1, S1 = canned bytes).
            let mut s0_s1_s2 = Vec::with_capacity(1 + HANDSHAKE_PAYLOAD_LEN * 2);
            s0_s1_s2.push(RTMP_VERSION);
            let s1 = [0xABu8; HANDSHAKE_PAYLOAD_LEN];
            s0_s1_s2.extend_from_slice(&s1);
            s0_s1_s2.extend_from_slice(&c1); // S2 echoes C1
            server.write_all(&s0_s1_s2).await.unwrap();

            // Read C2 — must equal our S1.
            let mut c2 = [0u8; HANDSHAKE_PAYLOAD_LEN];
            server.read_exact(&mut c2).await.unwrap();
            assert_eq!(c2, s1, "C2 must echo S1");
        });

        run_handshake(&mut client).await.unwrap();
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn handshake_rejects_wrong_s0_version() {
        let (mut client, mut server) = duplex(8192);

        tokio::spawn(async move {
            let mut buf = [0u8; 1 + HANDSHAKE_PAYLOAD_LEN];
            let _ = server.read_exact(&mut buf).await;
            // S0 = 0xFF (not 3).
            let mut bogus_s0_s1_s2 = vec![0xFFu8];
            bogus_s0_s1_s2.extend_from_slice(&[0u8; HANDSHAKE_PAYLOAD_LEN]);
            bogus_s0_s1_s2.extend_from_slice(&[0u8; HANDSHAKE_PAYLOAD_LEN]);
            let _ = server.write_all(&bogus_s0_s1_s2).await;
        });

        let err = run_handshake(&mut client).await.unwrap_err();
        assert!(err.to_string().contains("S0 returned version 255"));
    }

    #[tokio::test]
    async fn handshake_rejects_eof() {
        let (mut client, server) = duplex(8192);
        // Drop the server end immediately — client should fail reading
        // S0.
        drop(server);
        assert!(run_handshake(&mut client).await.is_err());
    }
}
