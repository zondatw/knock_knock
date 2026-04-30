//! TURN pinger — sends an Allocate Request (RFC 5766 §6.1) over UDP
//! and validates that the server answers with the expected
//! `401 Unauthorized` Allocate Error Response. We deliberately don't
//! supply credentials: the spec mandates that an unauthenticated
//! Allocate must be rejected with 401 and a REALM/NONCE attribute, so
//! that very rejection IS our success signal — it proves the TURN
//! server is alive and speaking the protocol correctly without us
//! having to allocate any actual relay state.
//!
//! Wire format reuses the STUN packet builder; only differences vs
//! `StunPinger`:
//! - Message Type = 0x0003 (Allocate Request) instead of 0x0001.
//! - Adds REQUESTED-TRANSPORT attribute (type 0x0019, length 4,
//!   value: 0x11000000 = UDP).
//! - Validates response Message Type 0x0113 (Allocate Error Response)
//!   AND that an ERROR-CODE attribute with code 401 is present.

use std::io::{self, Result};
use std::time::Duration;

use async_trait::async_trait;
use tokio::net::UdpSocket;

use crate::pinger::Pinger;
use crate::stun::{
    build_message, random_transaction_id, server_endpoint, validate_response_header,
    STUN_HEADER_LEN, STUN_TXID_LEN,
};
use crate::util::with_timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_PORT: u16 = 3478;
const BUF_SIZE: usize = 0xFF;

const MSG_ALLOCATE_REQUEST: u16 = 0x0003;
const MSG_ALLOCATE_ERROR: u16 = 0x0113;

const ATTR_ERROR_CODE: u16 = 0x0009;
const ATTR_REQUESTED_TRANSPORT: u16 = 0x0019;
const REQUESTED_TRANSPORT_UDP: u8 = 17; // IANA protocol number for UDP

/// TURN pinger — sends an unauthenticated Allocate Request to `server`
/// and considers the expected `401 Unauthorized` Allocate Error
/// Response a successful ping (it proves the server is alive and
/// speaks RFC 5766). Default port 3478. Doesn't actually allocate any
/// relay state, so it's safe to spam against shared TURN
/// infrastructure.
pub struct TurnPinger {
    pub server: String,
    pub timeout: Duration,
}

impl TurnPinger {
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
impl Pinger for TurnPinger {
    async fn ping(&self) -> Result<()> {
        let endpoint = server_endpoint(&self.server, DEFAULT_PORT)?;
        let txid = random_transaction_id();
        let attrs = build_requested_transport_attribute();
        let request = build_message(MSG_ALLOCATE_REQUEST, &attrs, &txid);

        with_timeout(self.timeout, async move {
            let socket = UdpSocket::bind("0.0.0.0:0").await?;
            socket.connect(&endpoint).await?;
            socket.send(&request).await?;

            let mut buf = [0u8; BUF_SIZE];
            let n = socket.recv(&mut buf).await?;
            validate_allocate_error_response(&buf[..n], &txid)
        })
        .await
    }
}

/// Build a single REQUESTED-TRANSPORT attribute (RFC 5766 §14.7):
/// 2-byte type 0x0019 | 2-byte length 4 | 1-byte protocol (UDP=17) |
/// 3 bytes RFFU (zero).
fn build_requested_transport_attribute() -> [u8; 8] {
    let mut attr = [0u8; 8];
    attr[0..2].copy_from_slice(&ATTR_REQUESTED_TRANSPORT.to_be_bytes());
    attr[2..4].copy_from_slice(&4u16.to_be_bytes());
    attr[4] = REQUESTED_TRANSPORT_UDP;
    // attr[5..8] remain zero (RFFU).
    attr
}

/// Validate the Allocate Error Response.
///
/// Required: 20-byte STUN header with Message Type 0x0113, magic
/// cookie intact, transaction ID echoed. Plus we walk the attributes
/// looking for an ERROR-CODE (0x0009) and confirm its code is 401.
/// REALM/NONCE presence isn't strictly required for a "server is
/// alive" decision — the 401 alone is the tell.
fn validate_allocate_error_response(buf: &[u8], expected_txid: &[u8; STUN_TXID_LEN]) -> Result<()> {
    validate_response_header(buf, MSG_ALLOCATE_ERROR, expected_txid)?;
    let body = &buf[STUN_HEADER_LEN..];
    match find_error_code(body)? {
        Some(401) => Ok(()),
        Some(other) => Err(io::Error::other(format!(
            "TURN Allocate Error returned code {other} (expected 401 Unauthorized)"
        ))),
        None => Err(io::Error::other(
            "TURN Allocate Error Response is missing the ERROR-CODE attribute",
        )),
    }
}

/// Walk the attribute TLV list looking for ERROR-CODE (0x0009) and
/// decode its `Class` (high 3 bits of byte 2) and `Number` (byte 3)
/// per RFC 5389 §15.6: returned status = Class * 100 + Number.
fn find_error_code(mut body: &[u8]) -> Result<Option<u16>> {
    while body.len() >= 4 {
        let attr_type = u16::from_be_bytes([body[0], body[1]]);
        let attr_len = u16::from_be_bytes([body[2], body[3]]) as usize;
        let total = 4 + attr_len;
        // Attributes are padded to the nearest 4-byte boundary.
        let padded = total.div_ceil(4) * 4;
        if body.len() < total {
            return Err(io::Error::other(
                "STUN attribute claims more bytes than the message contains",
            ));
        }
        if attr_type == ATTR_ERROR_CODE {
            if attr_len < 4 {
                return Err(io::Error::other(format!(
                    "STUN ERROR-CODE attribute too short ({attr_len} bytes, need >= 4)"
                )));
            }
            // bytes 0..2: reserved (must be 0); byte 2: low 3 bits =
            // class; byte 3: number.
            let class = u16::from(body[4 + 2] & 0x07);
            let number = u16::from(body[4 + 3]);
            return Ok(Some(class * 100 + number));
        }
        if body.len() < padded {
            // Last attribute, padding implicit (server may omit).
            break;
        }
        body = &body[padded..];
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stun::MAGIC_COOKIE;

    #[test]
    fn requested_transport_is_udp_17() {
        let attr = build_requested_transport_attribute();
        // Type
        assert_eq!(u16::from_be_bytes([attr[0], attr[1]]), 0x0019);
        // Length = 4
        assert_eq!(u16::from_be_bytes([attr[2], attr[3]]), 4);
        // UDP = 17
        assert_eq!(attr[4], 17);
        // RFFU
        assert_eq!(&attr[5..8], &[0, 0, 0]);
    }

    fn make_response_with_error(txid: &[u8; STUN_TXID_LEN], class: u8, number: u8) -> Vec<u8> {
        // Message body = ERROR-CODE attribute (4 bytes header + 4 bytes value)
        let mut body = Vec::new();
        body.extend_from_slice(&ATTR_ERROR_CODE.to_be_bytes());
        body.extend_from_slice(&4u16.to_be_bytes());
        body.push(0); // reserved
        body.push(0); // reserved
        body.push(class & 0x07);
        body.push(number);

        let mut packet = Vec::with_capacity(STUN_HEADER_LEN + body.len());
        packet.extend_from_slice(&MSG_ALLOCATE_ERROR.to_be_bytes());
        packet.extend_from_slice(&(body.len() as u16).to_be_bytes());
        packet.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        packet.extend_from_slice(txid);
        packet.extend_from_slice(&body);
        packet
    }

    #[test]
    fn validates_401_unauthorized() {
        let txid = [0xAA; STUN_TXID_LEN];
        let pkt = make_response_with_error(&txid, 4, 1);
        validate_allocate_error_response(&pkt, &txid).unwrap();
    }

    #[test]
    fn rejects_other_error_codes() {
        let txid = [0xAA; STUN_TXID_LEN];
        // 4xx but not 401 — e.g., 400 Bad Request.
        let pkt = make_response_with_error(&txid, 4, 0);
        assert!(validate_allocate_error_response(&pkt, &txid).is_err());
    }

    #[test]
    fn rejects_missing_error_code_attribute() {
        let txid = [0xAA; STUN_TXID_LEN];
        // Empty body — header valid, no ERROR-CODE.
        let mut pkt = Vec::with_capacity(STUN_HEADER_LEN);
        pkt.extend_from_slice(&MSG_ALLOCATE_ERROR.to_be_bytes());
        pkt.extend_from_slice(&0u16.to_be_bytes());
        pkt.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        pkt.extend_from_slice(&txid);
        assert!(validate_allocate_error_response(&pkt, &txid).is_err());
    }

    #[test]
    fn rejects_wrong_message_type() {
        let txid = [0xAA; STUN_TXID_LEN];
        let mut pkt = make_response_with_error(&txid, 4, 1);
        // Flip type to Allocate Success (0x0103) — that's not what an
        // unauthenticated Allocate should produce.
        pkt[0..2].copy_from_slice(&0x0103u16.to_be_bytes());
        assert!(validate_allocate_error_response(&pkt, &txid).is_err());
    }
}
