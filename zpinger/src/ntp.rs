//! NTP pinger — sends one NTP v4 client mode packet (RFC 5905 §7.3)
//! over UDP and validates the server response.
//!
//! Wire format (48 bytes, big-endian):
//! ```text
//! byte 0:    LI(2) | VN(3) | Mode(3) — 0x23 (LI=0, VN=4, Mode=3=client)
//! byte 1:    Stratum (0 in request)
//! byte 2:    Poll    (0)
//! byte 3:    Precision (0)
//! 4..8:      Root Delay      (0)
//! 8..12:     Root Dispersion (0)
//! 12..16:    Reference ID    (0)
//! 16..24:    Reference Timestamp (0)
//! 24..32:    Origin Timestamp    (0)
//! 32..40:    Receive Timestamp   (0)
//! 40..48:    Transmit Timestamp  (0 — server fills its own time)
//! ```
//! Validation: response is exactly 48 bytes; mode field (low 3 bits of
//! byte 0) is 4 (server) or 5 (broadcast); version field matches what
//! we sent (NTP servers echo the client's VN per RFC 5905 §7.3).

use std::io::{self, Result};
use std::time::Duration;

use async_trait::async_trait;
use tokio::net::UdpSocket;

use crate::pinger::Pinger;
use crate::uri::get_uri;
use crate::util::with_timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_PORT: u16 = 123;
const PACKET_LEN: usize = 48;
const BUF_SIZE: usize = 0xFF;

/// NTP version we speak. v4 is the current standard (RFC 5905);
/// servers also accept v3 packets and reply with v3 in kind, which is
/// why we validate the response VN against what we sent rather than
/// hard-coding 4.
const NTP_VERSION: u8 = 4;
const MODE_CLIENT: u8 = 3;
const MODE_SERVER: u8 = 4;
const MODE_BROADCAST: u8 = 5;

/// NTP pinger — sends a 48-byte NTP v4 client packet to `server` and
/// waits for a server reply. Default port 123. Doesn't decode the
/// timestamps — this is a "did the time server respond well-formedly"
/// probe, not a clock-discipline tool.
pub struct NtpPinger {
    pub server: String,
    pub timeout: Duration,
}

impl NtpPinger {
    pub fn new(server: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }
}

#[async_trait]
impl Pinger for NtpPinger {
    async fn ping(&self) -> Result<()> {
        let endpoint = server_endpoint(&self.server)?;
        let request = build_request();

        with_timeout(self.timeout, async move {
            let socket = UdpSocket::bind("0.0.0.0:0").await?;
            socket.connect(&endpoint).await?;
            socket.send(&request).await?;

            let mut buf = [0u8; BUF_SIZE];
            let n = socket.recv(&mut buf).await?;
            validate_response(&buf[..n])
        })
        .await
    }
}

fn server_endpoint(server: &str) -> Result<String> {
    let uri = get_uri(server);
    if uri.domain.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "NTP server target is missing a host",
        ));
    }
    let port = if uri.port > 0 {
        uri.port as u16
    } else {
        DEFAULT_PORT
    };
    Ok(format!("{}:{}", uri.domain, port))
}

/// Build a 48-byte NTP v4 client request. Every field besides the
/// LI/VN/Mode byte is zero — the server fills in its own timestamps
/// and references when generating the reply.
fn build_request() -> [u8; PACKET_LEN] {
    let mut packet = [0u8; PACKET_LEN];
    // LI(0) << 6 | VN(4) << 3 | Mode(3 = client)
    packet[0] = (NTP_VERSION << 3) | MODE_CLIENT;
    packet
}

fn validate_response(buf: &[u8]) -> Result<()> {
    if buf.len() != PACKET_LEN {
        return Err(io::Error::other(format!(
            "NTP response is {} bytes (expected {PACKET_LEN})",
            buf.len()
        )));
    }
    let first = buf[0];
    let mode = first & 0x07;
    if mode != MODE_SERVER && mode != MODE_BROADCAST {
        return Err(io::Error::other(format!(
            "NTP response mode {mode} is not server (4) or broadcast (5)"
        )));
    }
    let version = (first >> 3) & 0x07;
    if version != NTP_VERSION {
        return Err(io::Error::other(format!(
            "NTP response version {version} does not match request version {NTP_VERSION}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_packet_shape() {
        let p = build_request();
        assert_eq!(p.len(), PACKET_LEN);
        // VN=4 → bits 3..6 are 0b100; Mode=3 → bits 0..3 are 0b011.
        // 0b00_100_011 = 0x23.
        assert_eq!(p[0], 0x23);
        // Every other byte must be zero.
        assert!(p[1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn validate_response_accepts_well_formed_server_reply() {
        let mut buf = [0u8; PACKET_LEN];
        // VN=4, Mode=4 (server) → 0x24.
        buf[0] = (NTP_VERSION << 3) | MODE_SERVER;
        validate_response(&buf).unwrap();
    }

    #[test]
    fn validate_response_accepts_broadcast_mode() {
        let mut buf = [0u8; PACKET_LEN];
        buf[0] = (NTP_VERSION << 3) | MODE_BROADCAST;
        validate_response(&buf).unwrap();
    }

    #[test]
    fn validate_response_rejects_short_packet() {
        let buf = [0u8; PACKET_LEN - 1];
        assert!(validate_response(&buf).is_err());
    }

    #[test]
    fn validate_response_rejects_long_packet() {
        let buf = [0u8; PACKET_LEN + 1];
        assert!(validate_response(&buf).is_err());
    }

    #[test]
    fn validate_response_rejects_client_mode() {
        let mut buf = [0u8; PACKET_LEN];
        buf[0] = (NTP_VERSION << 3) | MODE_CLIENT;
        assert!(validate_response(&buf).is_err());
    }

    #[test]
    fn validate_response_rejects_version_mismatch() {
        let mut buf = [0u8; PACKET_LEN];
        // VN=2, Mode=4
        buf[0] = (2 << 3) | MODE_SERVER;
        assert!(validate_response(&buf).is_err());
    }

    #[test]
    fn server_endpoint_applies_default_port() {
        assert_eq!(
            server_endpoint("time.example.com").unwrap(),
            "time.example.com:123"
        );
    }

    #[test]
    fn server_endpoint_keeps_explicit_port() {
        assert_eq!(
            server_endpoint("time.example.com:1234").unwrap(),
            "time.example.com:1234"
        );
    }
}
