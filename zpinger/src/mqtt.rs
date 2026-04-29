//! MQTT 3.1.1 / 5 pinger — hand-rolled wire format, async I/O via
//! tokio. Zero new protocol-side deps; uses the same rustls layer as
//! HttpPinger for `mqtts://`.
//!
//! The "ping" runs the smallest meaningful MQTT exchange:
//! CONNECT → CONNACK → PINGREQ → PINGRESP → DISCONNECT. That covers
//! both the connection-establishment cost (network + broker handshake)
//! and the steady-state control-packet RTT, in line with how the
//! WebSocket pinger combines upgrade + PING/PONG.

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
const DEFAULT_PORT_PLAIN: u16 = 1883;
const DEFAULT_PORT_TLS: u16 = 8883;
const DEFAULT_KEEPALIVE: u16 = 60;

// MQTT control packet type codes (high nibble of fixed header byte 1).
const TYPE_CONNECT: u8 = 0x10;
const TYPE_CONNACK: u8 = 0x20;
const TYPE_PINGREQ: u8 = 0xC0;
const TYPE_PINGRESP: u8 = 0xD0;
const TYPE_DISCONNECT: u8 = 0xE0;

const MQTT_PROTOCOL_NAME: &[u8] = b"MQTT";
const CONNECT_FLAGS_CLEAN_SESSION: u8 = 0x02;

/// MQTT protocol version. Defaults to 3.1.1 (`MQTT-3.1.1-os`); pick
/// `V5` to advertise MQTT 5.0 in the CONNECT packet.
///
/// For the ping use case the wire difference is small: v5 raises the
/// protocol-level byte from 4 to 5 and adds an empty Properties
/// section to CONNECT. CONNACK / PINGREQ / PINGRESP / DISCONNECT
/// happen to be backward-compatible at the bytes we look at — both
/// versions put the success code at body[1], and a 2-byte
/// `[0xE0, 0x00]` DISCONNECT is legal in v5 too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MqttVersion {
    #[default]
    V3_1_1,
    V5,
}

impl MqttVersion {
    fn protocol_level(self) -> u8 {
        match self {
            MqttVersion::V3_1_1 => 4,
            MqttVersion::V5 => 5,
        }
    }
}

/// MQTT pinger.
///
/// Speaks plain TCP for `mqtt://` (or no scheme) targets and rustls
/// TLS for `mqtts://`. Reuses zpinger's webpki-roots default trust
/// store unless a caller-supplied `ClientConfig` is injected via
/// `with_tls_config` — same pattern as `HttpPinger` / `WebSocketPinger`.
pub struct MqttPinger {
    pub server: String,
    pub client_id: Option<String>,
    pub keepalive: u16,
    pub timeout: Duration,
    pub version: MqttVersion,
    tls_config: Option<Arc<ClientConfig>>,
}

impl MqttPinger {
    pub fn new(server: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            client_id: None,
            keepalive: DEFAULT_KEEPALIVE,
            timeout: DEFAULT_TIMEOUT,
            version: MqttVersion::default(),
            tls_config: None,
        }
    }

    pub fn with_client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = Some(client_id.into());
        self
    }

    pub fn with_keepalive(mut self, keepalive: u16) -> Self {
        self.keepalive = keepalive;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_version(mut self, version: MqttVersion) -> Self {
        self.version = version;
        self
    }

    pub fn with_tls_config(mut self, config: Arc<ClientConfig>) -> Self {
        self.tls_config = Some(config);
        self
    }

    async fn ping_plain(&self, uri: &URI) -> Result<()> {
        let endpoint = endpoint_for(uri, DEFAULT_PORT_PLAIN)?;
        let client_id = self.client_id.clone();
        let keepalive = self.keepalive;
        let version = self.version;
        with_timeout(self.timeout, async move {
            let mut stream = TcpStream::connect(&endpoint).await?;
            run_session(&mut stream, &client_id, keepalive, version).await
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
        let client_id = self.client_id.clone();
        let keepalive = self.keepalive;
        let version = self.version;
        with_timeout(self.timeout, async move {
            let tcp = TcpStream::connect(&endpoint).await?;
            let connector = TlsConnector::from(config);
            let mut stream = connector.connect(server_name, tcp).await?;
            run_session(&mut stream, &client_id, keepalive, version).await
        })
        .await
    }
}

#[async_trait]
impl Pinger for MqttPinger {
    async fn ping(&self) -> Result<()> {
        let uri = get_uri(&self.server);
        let scheme = uri.scheme.to_ascii_lowercase();
        match scheme.as_str() {
            "" | "mqtt" => self.ping_plain(&uri).await,
            "mqtts" => self.ping_tls(&uri).await,
            other => Err(io::Error::other(format!(
                "scheme '{other}' is not supported by MqttPinger (use mqtt:// or mqtts://)"
            ))),
        }
    }
}

fn endpoint_for(uri: &URI, default_port: u16) -> Result<String> {
    if uri.domain.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing host in MQTT broker URL",
        ));
    }
    let port = if uri.port > 0 {
        uri.port as u16
    } else {
        default_port
    };
    Ok(format!("{}:{}", uri.domain, port))
}

fn default_client_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("knockknock-{nanos:08x}")
}

/// Drive a full ping session over an established stream.
async fn run_session<S>(
    stream: &mut S,
    client_id: &Option<String>,
    keepalive: u16,
    version: MqttVersion,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let cid_owned;
    let cid: &str = match client_id {
        Some(c) => c.as_str(),
        None => {
            cid_owned = default_client_id();
            cid_owned.as_str()
        }
    };

    let connect = build_connect(cid, keepalive, version);
    stream.write_all(&connect).await?;

    let connack = read_packet(stream).await?;
    validate_connack(&connack)?;

    stream.write_all(&[TYPE_PINGREQ, 0x00]).await?;

    let pingresp = read_packet(stream).await?;
    validate_pingresp(&pingresp)?;

    let _ = stream.write_all(&[TYPE_DISCONNECT, 0x00]).await;

    Ok(())
}

/// One MQTT control packet, split into its type byte and body.
struct MqttPacket {
    packet_type: u8,
    body: Vec<u8>,
}

fn build_connect(client_id: &str, keepalive: u16, version: MqttVersion) -> Vec<u8> {
    let cid_bytes = client_id.as_bytes();

    let mut variable_header = Vec::with_capacity(12);
    variable_header.extend_from_slice(&(MQTT_PROTOCOL_NAME.len() as u16).to_be_bytes());
    variable_header.extend_from_slice(MQTT_PROTOCOL_NAME);
    variable_header.push(version.protocol_level());
    variable_header.push(CONNECT_FLAGS_CLEAN_SESSION);
    variable_header.extend_from_slice(&keepalive.to_be_bytes());
    if version == MqttVersion::V5 {
        // MQTT 5 §3.1.2.11 — Properties section. We don't set any
        // properties for the ping, but the section itself is required;
        // a single byte 0x00 is the varint encoding of length 0.
        variable_header.push(0x00);
    }

    let mut payload = Vec::with_capacity(2 + cid_bytes.len());
    payload.extend_from_slice(&(cid_bytes.len() as u16).to_be_bytes());
    payload.extend_from_slice(cid_bytes);

    let body_len = variable_header.len() + payload.len();
    let remaining = encode_varint(body_len);

    let mut packet = Vec::with_capacity(1 + remaining.len() + body_len);
    packet.push(TYPE_CONNECT);
    packet.extend_from_slice(&remaining);
    packet.extend_from_slice(&variable_header);
    packet.extend_from_slice(&payload);
    packet
}

fn encode_varint(n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut value = n;
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return out;
        }
    }
}

async fn read_packet<S>(stream: &mut S) -> Result<MqttPacket>
where
    S: AsyncRead + Unpin,
{
    let mut header = [0u8; 1];
    stream.read_exact(&mut header).await?;

    let mut multiplier: usize = 1;
    let mut remaining: usize = 0;
    for _ in 0..4 {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await?;
        let b = byte[0];
        remaining += (b & 0x7F) as usize * multiplier;
        if b & 0x80 == 0 {
            let mut body = vec![0u8; remaining];
            if remaining > 0 {
                stream.read_exact(&mut body).await?;
            }
            return Ok(MqttPacket {
                packet_type: header[0],
                body,
            });
        }
        multiplier = multiplier.saturating_mul(128);
    }
    Err(io::Error::other(
        "MQTT remaining-length varint exceeds 4 bytes",
    ))
}

fn validate_connack(packet: &MqttPacket) -> Result<()> {
    if packet.packet_type & 0xF0 != TYPE_CONNACK {
        return Err(io::Error::other(format!(
            "expected CONNACK (0x20), got packet type {:#x}",
            packet.packet_type
        )));
    }
    if packet.body.len() < 2 {
        return Err(io::Error::other(
            "CONNACK body shorter than 2 bytes (flags + return code)",
        ));
    }
    // body[0] = session-present flags (we don't care for ping)
    let return_code = packet.body[1];
    if return_code != 0 {
        return Err(io::Error::other(format!(
            "broker rejected CONNECT with return code {return_code}"
        )));
    }
    Ok(())
}

fn validate_pingresp(packet: &MqttPacket) -> Result<()> {
    if packet.packet_type & 0xF0 != TYPE_PINGRESP {
        return Err(io::Error::other(format!(
            "expected PINGRESP (0xD0), got packet type {:#x}",
            packet.packet_type
        )));
    }
    if !packet.body.is_empty() {
        return Err(io::Error::other("PINGRESP must carry no payload"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_one_byte() {
        assert_eq!(encode_varint(0), vec![0x00]);
        assert_eq!(encode_varint(1), vec![0x01]);
        assert_eq!(encode_varint(127), vec![0x7F]);
    }

    #[test]
    fn varint_two_bytes() {
        // 128 → 0x80 0x01 per RFC §2.2.3
        assert_eq!(encode_varint(128), vec![0x80, 0x01]);
        assert_eq!(encode_varint(16383), vec![0xFF, 0x7F]);
    }

    #[test]
    fn build_connect_v3_1_1_smallest_client_id() {
        let pkt = build_connect("a", 30, MqttVersion::V3_1_1);
        // remaining length = 10 (var header) + 3 (cid u16 len + 1 byte) = 13
        // total = 1 (type) + 1 (varint) + 13 = 15 bytes
        assert_eq!(pkt.len(), 15);
        assert_eq!(pkt[0], TYPE_CONNECT);
        assert_eq!(pkt[1], 13);
        assert_eq!(&pkt[2..4], &(MQTT_PROTOCOL_NAME.len() as u16).to_be_bytes());
        assert_eq!(&pkt[4..8], MQTT_PROTOCOL_NAME);
        assert_eq!(pkt[8], 4); // protocol level 4 = MQTT 3.1.1
        assert_eq!(pkt[9], CONNECT_FLAGS_CLEAN_SESSION);
        assert_eq!(&pkt[10..12], &30u16.to_be_bytes());
        assert_eq!(&pkt[12..14], &1u16.to_be_bytes());
        assert_eq!(pkt[14], b'a');
    }

    #[test]
    fn build_connect_v5_includes_empty_properties() {
        let pkt = build_connect("a", 30, MqttVersion::V5);
        // v5 inserts one extra byte (properties length varint = 0x00)
        // between keepalive and the payload, so the packet is 1 byte
        // longer than the v3.1.1 case.
        assert_eq!(pkt.len(), 16);
        assert_eq!(pkt[0], TYPE_CONNECT);
        assert_eq!(pkt[1], 14);
        assert_eq!(pkt[8], 5); // protocol level 5 = MQTT 5
        assert_eq!(pkt[9], CONNECT_FLAGS_CLEAN_SESSION);
        assert_eq!(&pkt[10..12], &30u16.to_be_bytes());
        assert_eq!(pkt[12], 0x00); // properties length = 0
        assert_eq!(&pkt[13..15], &1u16.to_be_bytes());
        assert_eq!(pkt[15], b'a');
    }

    #[test]
    fn validate_connack_v5_with_properties_tail_is_accepted() {
        // A v5 broker may put a Properties section after the reason
        // code. validate_connack only inspects body[0..2], so the
        // tail bytes don't matter for our success check.
        let p = MqttPacket {
            packet_type: TYPE_CONNACK,
            body: vec![0x00, 0x00, 0x05, 0x11, 0x00, 0x00, 0x00, 0x10],
        };
        validate_connack(&p).unwrap();
    }

    async fn read_one(bytes: Vec<u8>) -> MqttPacket {
        let mut cur = std::io::Cursor::new(bytes);
        read_packet(&mut cur).await.unwrap()
    }

    #[tokio::test]
    async fn read_packet_roundtrip() {
        let bytes = vec![TYPE_CONNACK, 0x02, 0x00, 0x00];
        let p = read_one(bytes).await;
        assert_eq!(p.packet_type, TYPE_CONNACK);
        assert_eq!(p.body, vec![0x00, 0x00]);
    }

    #[tokio::test]
    async fn read_packet_zero_body() {
        let bytes = vec![TYPE_PINGRESP, 0x00];
        let p = read_one(bytes).await;
        assert_eq!(p.packet_type, TYPE_PINGRESP);
        assert!(p.body.is_empty());
    }

    #[test]
    fn validate_connack_accepts_zero_return_code() {
        let p = MqttPacket {
            packet_type: TYPE_CONNACK,
            body: vec![0x00, 0x00],
        };
        validate_connack(&p).unwrap();
    }

    #[test]
    fn validate_connack_rejects_nonzero_return_code() {
        let p = MqttPacket {
            packet_type: TYPE_CONNACK,
            body: vec![0x00, 0x05],
        };
        assert!(validate_connack(&p).is_err());
    }

    #[test]
    fn validate_connack_rejects_wrong_packet_type() {
        let p = MqttPacket {
            packet_type: TYPE_PINGRESP,
            body: vec![0x00, 0x00],
        };
        assert!(validate_connack(&p).is_err());
    }

    #[test]
    fn validate_pingresp_accepts_empty_body() {
        let p = MqttPacket {
            packet_type: TYPE_PINGRESP,
            body: Vec::new(),
        };
        validate_pingresp(&p).unwrap();
    }

    #[test]
    fn validate_pingresp_rejects_payload() {
        let p = MqttPacket {
            packet_type: TYPE_PINGRESP,
            body: vec![0xFF],
        };
        assert!(validate_pingresp(&p).is_err());
    }

    #[test]
    fn validate_pingresp_rejects_wrong_type() {
        let p = MqttPacket {
            packet_type: TYPE_CONNACK,
            body: Vec::new(),
        };
        assert!(validate_pingresp(&p).is_err());
    }

    #[test]
    fn endpoint_for_applies_default_port() {
        let uri = get_uri("broker.example.com");
        assert_eq!(
            endpoint_for(&uri, DEFAULT_PORT_PLAIN).unwrap(),
            "broker.example.com:1883"
        );
    }

    #[test]
    fn endpoint_for_keeps_explicit_port() {
        let uri = get_uri("mqtt://broker.example.com:11883");
        assert_eq!(
            endpoint_for(&uri, DEFAULT_PORT_PLAIN).unwrap(),
            "broker.example.com:11883"
        );
    }

    #[test]
    fn endpoint_for_rejects_empty_host() {
        let uri = get_uri("");
        assert!(endpoint_for(&uri, DEFAULT_PORT_PLAIN).is_err());
    }
}
