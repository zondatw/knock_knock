//! STUN pinger — sends one Binding Request (RFC 5389 §6) over UDP and
//! validates the Binding Success Response.
//!
//! Wire format (20-byte STUN header, big-endian):
//! ```text
//! bytes 0..2:   Message Type    (0x0001 = Binding Request)
//! bytes 2..4:   Message Length  (0x0000 — no attributes)
//! bytes 4..8:   Magic Cookie    (0x2112A442)
//! bytes 8..20:  Transaction ID  (96-bit random)
//! ```
//! Validation: response is at least 20 bytes; message type is 0x0101
//! (Binding Success Response); magic cookie matches; transaction ID
//! is the same one we generated.

use std::io::{self, Result};
#[cfg(feature = "stun")]
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "stun")]
use async_trait::async_trait;
#[cfg(feature = "stun")]
use tokio::net::UdpSocket;

#[cfg(feature = "stun")]
use crate::pinger::Pinger;
use crate::uri::get_uri;
#[cfg(feature = "stun")]
use crate::util::with_timeout;

#[cfg(feature = "stun")]
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(feature = "stun")]
const DEFAULT_PORT: u16 = 3478;
#[cfg(feature = "stun")]
const BUF_SIZE: usize = 0xFF;

pub(crate) const STUN_HEADER_LEN: usize = 20;
pub(crate) const STUN_TXID_LEN: usize = 12;
pub(crate) const MAGIC_COOKIE: u32 = 0x2112_A442;

#[cfg(feature = "stun")]
const MSG_BINDING_REQUEST: u16 = 0x0001;
#[cfg(feature = "stun")]
const MSG_BINDING_SUCCESS: u16 = 0x0101;

/// STUN pinger — sends a Binding Request to `server` and waits for a
/// Binding Success Response. Default port 3478. Doesn't extract the
/// XOR-MAPPED-ADDRESS attribute — this is a "did the STUN server
/// answer correctly" probe, not a NAT-mapping discovery tool.
#[cfg(feature = "stun")]
pub struct StunPinger {
    pub server: String,
    pub timeout: Duration,
}

#[cfg(feature = "stun")]
impl StunPinger {
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

#[cfg(feature = "stun")]
#[async_trait]
impl Pinger for StunPinger {
    async fn ping(&self) -> Result<()> {
        let endpoint = server_endpoint(&self.server, DEFAULT_PORT)?;
        let txid = random_transaction_id();
        let request = build_header(MSG_BINDING_REQUEST, 0, &txid);

        with_timeout(self.timeout, async move {
            let socket = UdpSocket::bind("0.0.0.0:0").await?;
            socket.connect(&endpoint).await?;
            socket.send(&request).await?;

            let mut buf = [0u8; BUF_SIZE];
            let n = socket.recv(&mut buf).await?;
            validate_binding_response(&buf[..n], &txid)
        })
        .await
    }
}

pub(crate) fn server_endpoint(server: &str, default_port: u16) -> Result<String> {
    let uri = get_uri(server);
    if uri.domain.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "STUN/TURN server target is missing a host",
        ));
    }
    let port = if uri.port > 0 {
        uri.port as u16
    } else {
        default_port
    };
    Ok(format!("{}:{}", uri.domain, port))
}

/// Build a STUN message: 20-byte header followed by `attributes` (may
/// be empty for a vanilla Binding Request). `message_type` is one of
/// the RFC 5389 / RFC 5766 message-type codes; `attributes` is already
/// in TLV-encoded form so the caller controls padding.
#[cfg(feature = "turn")]
pub(crate) fn build_message(
    message_type: u16,
    attributes: &[u8],
    transaction_id: &[u8; STUN_TXID_LEN],
) -> Vec<u8> {
    let mut packet = Vec::with_capacity(STUN_HEADER_LEN + attributes.len());
    let length = u16::try_from(attributes.len()).expect("STUN attributes fit in u16");
    packet.extend_from_slice(&build_header(message_type, length, transaction_id));
    packet.extend_from_slice(attributes);
    packet
}

#[cfg(any(feature = "stun", feature = "turn"))]
fn build_header(
    message_type: u16,
    message_length: u16,
    transaction_id: &[u8; STUN_TXID_LEN],
) -> [u8; STUN_HEADER_LEN] {
    let mut header = [0u8; STUN_HEADER_LEN];
    header[0..2].copy_from_slice(&message_type.to_be_bytes());
    header[2..4].copy_from_slice(&message_length.to_be_bytes());
    header[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
    header[8..20].copy_from_slice(transaction_id);
    header
}

/// Generate a 96-bit transaction ID. RFC 5389 §6 requires this to be
/// "uniformly and randomly chosen from the interval 0 .. 2^96-1"; we
/// derive it cheaply from the system clock + per-call entropy. No
/// crypto attacker model here — this is just collision avoidance for
/// concurrent pings to the same server.
pub(crate) fn random_transaction_id() -> [u8; STUN_TXID_LEN] {
    let mut id = [0u8; STUN_TXID_LEN];
    if let Ok(d) = SystemTime::now().duration_since(UNIX_EPOCH) {
        let secs = d.as_secs();
        let nanos = u64::from(d.subsec_nanos());
        id[0..8].copy_from_slice(&secs.to_be_bytes());
        id[8..12].copy_from_slice(&(nanos as u32).to_be_bytes());
    }
    id
}

/// Validate a Binding Success Response. Strict by design: any deviation
/// from RFC 5389 §6 in the header is treated as a failed ping.
#[cfg(feature = "stun")]
fn validate_binding_response(buf: &[u8], expected_txid: &[u8; STUN_TXID_LEN]) -> Result<()> {
    validate_response_header(buf, MSG_BINDING_SUCCESS, expected_txid)
}

/// Shared header validation used by both `StunPinger` (success
/// response) and `TurnPinger` (error response). Checks length, message
/// type, magic cookie, and transaction ID echo.
pub(crate) fn validate_response_header(
    buf: &[u8],
    expected_type: u16,
    expected_txid: &[u8; STUN_TXID_LEN],
) -> Result<()> {
    if buf.len() < STUN_HEADER_LEN {
        return Err(io::Error::other(format!(
            "STUN response shorter than 20-byte header (got {} bytes)",
            buf.len()
        )));
    }
    let message_type = u16::from_be_bytes([buf[0], buf[1]]);
    if message_type != expected_type {
        return Err(io::Error::other(format!(
            "STUN response message type {message_type:#06x} (expected {expected_type:#06x})"
        )));
    }
    let cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    if cookie != MAGIC_COOKIE {
        return Err(io::Error::other(format!(
            "STUN response magic cookie {cookie:#010x} does not match {MAGIC_COOKIE:#010x}"
        )));
    }
    if &buf[8..20] != expected_txid.as_slice() {
        return Err(io::Error::other(
            "STUN response transaction ID does not match request",
        ));
    }
    Ok(())
}

// Tests need both `build_message` (turn-gated) and the binding-response
// validators (stun-gated) in scope, so we gate the whole test module on
// the union. `cargo test --workspace` runs with the default `all`
// feature set, where both are on.
#[cfg(all(test, feature = "stun", feature = "turn"))]
mod tests {
    use super::*;

    #[test]
    fn build_message_no_attributes() {
        let txid = [0xAB; STUN_TXID_LEN];
        let pkt = build_message(MSG_BINDING_REQUEST, &[], &txid);
        assert_eq!(pkt.len(), STUN_HEADER_LEN);
        assert_eq!(&pkt[0..2], &MSG_BINDING_REQUEST.to_be_bytes());
        assert_eq!(&pkt[2..4], &0u16.to_be_bytes());
        assert_eq!(&pkt[4..8], &MAGIC_COOKIE.to_be_bytes());
        assert_eq!(&pkt[8..20], &txid);
    }

    #[test]
    fn build_message_with_attributes() {
        let txid = [0; STUN_TXID_LEN];
        let attrs = [1, 2, 3, 4];
        let pkt = build_message(0x0003, &attrs, &txid);
        assert_eq!(pkt.len(), STUN_HEADER_LEN + 4);
        // Length field reflects attribute byte count.
        assert_eq!(u16::from_be_bytes([pkt[2], pkt[3]]), 4);
        assert_eq!(&pkt[STUN_HEADER_LEN..], &attrs);
    }

    #[test]
    fn validate_binding_response_accepts_well_formed() {
        let txid = [0x55; STUN_TXID_LEN];
        let mut buf = [0u8; STUN_HEADER_LEN];
        buf[0..2].copy_from_slice(&MSG_BINDING_SUCCESS.to_be_bytes());
        buf[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        buf[8..20].copy_from_slice(&txid);
        validate_binding_response(&buf, &txid).unwrap();
    }

    #[test]
    fn validate_binding_response_rejects_short_buffer() {
        let txid = [0u8; STUN_TXID_LEN];
        let buf = [0u8; STUN_HEADER_LEN - 1];
        assert!(validate_binding_response(&buf, &txid).is_err());
    }

    #[test]
    fn validate_binding_response_rejects_wrong_type() {
        let txid = [0x55; STUN_TXID_LEN];
        let mut buf = [0u8; STUN_HEADER_LEN];
        // Send back a Binding Request (0x0001) instead of Success
        // (0x0101) — must be rejected.
        buf[0..2].copy_from_slice(&MSG_BINDING_REQUEST.to_be_bytes());
        buf[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        buf[8..20].copy_from_slice(&txid);
        assert!(validate_binding_response(&buf, &txid).is_err());
    }

    #[test]
    fn validate_binding_response_rejects_bad_cookie() {
        let txid = [0x55; STUN_TXID_LEN];
        let mut buf = [0u8; STUN_HEADER_LEN];
        buf[0..2].copy_from_slice(&MSG_BINDING_SUCCESS.to_be_bytes());
        buf[4..8].copy_from_slice(&0xDEAD_BEEFu32.to_be_bytes());
        buf[8..20].copy_from_slice(&txid);
        assert!(validate_binding_response(&buf, &txid).is_err());
    }

    #[test]
    fn validate_binding_response_rejects_txid_mismatch() {
        let sent = [0x55; STUN_TXID_LEN];
        let other = [0x66; STUN_TXID_LEN];
        let mut buf = [0u8; STUN_HEADER_LEN];
        buf[0..2].copy_from_slice(&MSG_BINDING_SUCCESS.to_be_bytes());
        buf[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        buf[8..20].copy_from_slice(&other);
        assert!(validate_binding_response(&buf, &sent).is_err());
    }

    #[test]
    fn server_endpoint_applies_default_port() {
        assert_eq!(
            server_endpoint("stun.example.com", DEFAULT_PORT).unwrap(),
            "stun.example.com:3478"
        );
    }

    #[test]
    fn server_endpoint_keeps_explicit_port() {
        assert_eq!(
            server_endpoint("stun.example.com:19302", DEFAULT_PORT).unwrap(),
            "stun.example.com:19302"
        );
    }
}
