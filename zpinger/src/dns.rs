use std::io::{self, Result};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::net::UdpSocket;

use crate::level4::with_timeout;
use crate::pinger::Pinger;
use crate::uri::get_uri;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_PORT: u16 = 53;
const BUF_SIZE: usize = 512;

/// Subset of DNS resource-record TYPE codes (RFC 1035 + extensions).
/// Only the ones a "speed test" CLI is likely to want; not exhaustive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordType {
    A,
    Aaaa,
    Cname,
    Mx,
    Ns,
    Txt,
}

impl RecordType {
    fn code(self) -> u16 {
        match self {
            RecordType::A => 1,
            RecordType::Ns => 2,
            RecordType::Cname => 5,
            RecordType::Mx => 15,
            RecordType::Txt => 16,
            RecordType::Aaaa => 28,
        }
    }
}

/// DNS pinger — sends one UDP query to `server`, waits for a
/// well-formed response, and reports the round trip. The query
/// content (`query` + `record_type`) is sent as-is; the response is
/// validated only structurally (matching ID, response bit set, RCODE
/// = NoError). Whether the answer section carries useful records is
/// not checked — this is a "did the server respond" probe, not a
/// resolver.
pub struct DnsPinger {
    pub server: String,
    pub query: String,
    pub record_type: RecordType,
    pub timeout: Duration,
}

impl DnsPinger {
    pub fn new(server: impl Into<String>, query: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            query: query.into(),
            record_type: RecordType::A,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_record_type(mut self, record_type: RecordType) -> Self {
        self.record_type = record_type;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait]
impl Pinger for DnsPinger {
    async fn ping(&self) -> Result<()> {
        if self.query.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "DNS query name is empty",
            ));
        }

        let endpoint = server_endpoint(&self.server)?;
        let id = random_id();
        let request = build_query(id, &self.query, self.record_type.code())?;

        with_timeout(self.timeout, async move {
            let socket = UdpSocket::bind("0.0.0.0:0").await?;
            socket.connect(&endpoint).await?;
            socket.send(&request).await?;

            let mut buf = [0u8; BUF_SIZE];
            let n = socket.recv(&mut buf).await?;
            validate_response(&buf[..n], &request, id)
        })
        .await
    }
}

fn server_endpoint(server: &str) -> Result<String> {
    let uri = get_uri(server);
    if uri.domain.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "DNS server target is missing a host",
        ));
    }
    let port = if uri.port > 0 {
        uri.port as u16
    } else {
        DEFAULT_PORT
    };
    Ok(format!("{}:{}", uri.domain, port))
}

fn random_id() -> u16 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| {
            let nanos = d.subsec_nanos();
            let secs = d.as_secs() as u32;
            (nanos ^ secs) as u16
        })
        .unwrap_or(0)
}

/// Encode a domain name as a DNS-format question name: a sequence of
/// length-prefixed labels terminated by a zero byte.
fn encode_name(name: &str) -> Result<Vec<u8>> {
    let trimmed = name.trim_end_matches('.');
    let mut out = Vec::with_capacity(trimmed.len() + 2);
    for label in trimmed.split('.') {
        if label.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "DNS query name has an empty label",
            ));
        }
        if label.len() > 63 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "DNS label exceeds 63 bytes",
            ));
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    if out.len() > 255 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "encoded DNS name exceeds 255 bytes",
        ));
    }
    Ok(out)
}

fn build_query(id: u16, name: &str, qtype: u16) -> Result<Vec<u8>> {
    let qname = encode_name(name)?;
    let mut packet = Vec::with_capacity(12 + qname.len() + 4);
    // Header
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&0x0100u16.to_be_bytes()); // standard query, recursion desired
    packet.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    packet.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    packet.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    packet.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
                                                   // Question
    packet.extend_from_slice(&qname);
    packet.extend_from_slice(&qtype.to_be_bytes());
    packet.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = IN
    Ok(packet)
}

fn validate_response(buf: &[u8], request: &[u8], expected_id: u16) -> Result<()> {
    if buf.len() < 12 {
        return Err(io::Error::other("DNS response shorter than header"));
    }
    let resp_id = u16::from_be_bytes([buf[0], buf[1]]);
    if resp_id != expected_id {
        return Err(io::Error::other(format!(
            "DNS response ID {resp_id:#x} does not match query ID {expected_id:#x}"
        )));
    }
    let flags = u16::from_be_bytes([buf[2], buf[3]]);
    let qr = flags >> 15; // top bit = response flag
    if qr != 1 {
        return Err(io::Error::other(
            "DNS response QR flag not set (server returned a query)",
        ));
    }
    let rcode = flags & 0x000F;
    if rcode != 0 {
        return Err(io::Error::other(format!(
            "DNS server returned RCODE {rcode} (non-zero = error)"
        )));
    }

    // RFC 1035 §4.1.2: the response repeats the question section
    // verbatim. Servers can't apply name compression at the start of
    // the message (compression pointers only refer backwards), so a
    // byte-for-byte comparison is valid and catches servers that
    // process a different query than the one we asked for.
    let qdcount = u16::from_be_bytes([buf[4], buf[5]]);
    if qdcount != 1 {
        return Err(io::Error::other(format!(
            "DNS response QDCOUNT = {qdcount} (expected 1)"
        )));
    }
    let question = &request[12..]; // request header is exactly 12 bytes
    if buf.len() < 12 + question.len() {
        return Err(io::Error::other(
            "DNS response too short to carry an echoed question section",
        ));
    }
    if &buf[12..12 + question.len()] != question {
        return Err(io::Error::other(
            "DNS response question section does not match the query",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_name_simple() {
        let bytes = encode_name("example.com").unwrap();
        assert_eq!(
            bytes,
            vec![7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0]
        );
    }

    #[test]
    fn encode_name_strips_trailing_dot() {
        let with_dot = encode_name("example.com.").unwrap();
        let without_dot = encode_name("example.com").unwrap();
        assert_eq!(with_dot, without_dot);
    }

    #[test]
    fn encode_name_rejects_empty_label() {
        assert!(encode_name("foo..bar").is_err());
    }

    #[test]
    fn encode_name_rejects_oversize_label() {
        let label = "a".repeat(64);
        assert!(encode_name(&label).is_err());
    }

    #[test]
    fn build_query_lengths() {
        let q = build_query(0x1234, "a.b", 1).unwrap();
        // 12 header + 5 qname (1+1+1+1+0+1) + 4 qtype/qclass = 21 ... let me recount
        // qname: [1]a[1]b[0] = 5 bytes
        assert_eq!(q.len(), 12 + 5 + 4);
        assert_eq!(&q[0..2], &0x1234u16.to_be_bytes());
        assert_eq!(&q[2..4], &0x0100u16.to_be_bytes()); // RD
        assert_eq!(&q[4..6], &1u16.to_be_bytes()); // QDCOUNT = 1
    }

    /// Helper: build a well-formed response packet that echoes the
    /// given request and lets tests poke specific fields to make
    /// it bad.
    fn well_formed_response(request: &[u8], id: u16) -> Vec<u8> {
        let mut buf = request.to_vec();
        buf[0..2].copy_from_slice(&id.to_be_bytes());
        buf[2..4].copy_from_slice(&0x8000u16.to_be_bytes()); // QR=1, RCODE=0
        buf
    }

    #[test]
    fn validate_response_rejects_short_buffer() {
        let request = build_query(0xDEAD, "a.b", 1).unwrap();
        assert!(validate_response(&[0u8; 5], &request, 0).is_err());
    }

    #[test]
    fn validate_response_rejects_id_mismatch() {
        let request = build_query(0xDEAD, "a.b", 1).unwrap();
        let mut buf = well_formed_response(&request, 0xBEEF);
        buf[2..4].copy_from_slice(&0x8000u16.to_be_bytes());
        assert!(validate_response(&buf, &request, 0xDEAD).is_err());
    }

    #[test]
    fn validate_response_rejects_query_qr() {
        let request = build_query(0xDEAD, "a.b", 1).unwrap();
        let mut buf = well_formed_response(&request, 0xDEAD);
        buf[2..4].copy_from_slice(&0u16.to_be_bytes()); // QR = 0
        assert!(validate_response(&buf, &request, 0xDEAD).is_err());
    }

    #[test]
    fn validate_response_rejects_nonzero_rcode() {
        let request = build_query(0xDEAD, "a.b", 1).unwrap();
        let mut buf = well_formed_response(&request, 0xDEAD);
        buf[2..4].copy_from_slice(&0x8003u16.to_be_bytes()); // QR=1, RCODE=3
        assert!(validate_response(&buf, &request, 0xDEAD).is_err());
    }

    #[test]
    fn validate_response_rejects_question_mismatch() {
        let request = build_query(0xDEAD, "a.b", 1).unwrap();
        let mut buf = well_formed_response(&request, 0xDEAD);
        // flip a byte inside the question section (qname's first label)
        buf[13] ^= 0xFF;
        assert!(validate_response(&buf, &request, 0xDEAD).is_err());
    }

    #[test]
    fn validate_response_rejects_zero_qdcount() {
        let request = build_query(0xDEAD, "a.b", 1).unwrap();
        let mut buf = well_formed_response(&request, 0xDEAD);
        buf[4..6].copy_from_slice(&0u16.to_be_bytes()); // QDCOUNT = 0
        assert!(validate_response(&buf, &request, 0xDEAD).is_err());
    }

    #[test]
    fn validate_response_rejects_truncated_question() {
        let request = build_query(0xDEAD, "a.b", 1).unwrap();
        let buf = well_formed_response(&request, 0xDEAD);
        // chop off the qtype/qclass tail of the response so the
        // question section is incomplete.
        let cut = &buf[..buf.len() - 2];
        assert!(validate_response(cut, &request, 0xDEAD).is_err());
    }

    #[test]
    fn validate_response_accepts_well_formed() {
        let request = build_query(0xDEAD, "a.b", 1).unwrap();
        let buf = well_formed_response(&request, 0xDEAD);
        validate_response(&buf, &request, 0xDEAD).unwrap();
    }

    #[test]
    fn server_endpoint_applies_default_port() {
        assert_eq!(server_endpoint("8.8.8.8").unwrap(), "8.8.8.8:53");
    }

    #[test]
    fn server_endpoint_keeps_explicit_port() {
        assert_eq!(server_endpoint("8.8.8.8:5353").unwrap(), "8.8.8.8:5353");
    }
}
