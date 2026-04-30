use std::net::SocketAddr;

#[path = "tests/test_pinger.rs"]
#[cfg(test)]
mod test_pinger;

// Always compiled regardless of features — the `Pinger` trait, the
// timed helper, the URI parser, and shared utilities. No protocol
// implementations live in here.
mod pinger;
pub mod uri;
mod util;

pub use crate::pinger::{timed, Pinger};

// TLS layer + rustls re-exports. Compiled whenever any protocol that
// needs TLS is enabled (http / ws / mqtt / hls).
#[cfg(feature = "_tls")]
mod tls;
#[cfg(feature = "_tls")]
pub use crate::tls::default_client_config;
#[cfg(feature = "_tls")]
pub use rustls::ClientConfig;

// Per-protocol modules + re-exports — each gated behind its own
// feature flag.
#[cfg(any(feature = "tcp", feature = "udp"))]
mod level4;
#[cfg(feature = "tcp")]
pub use crate::level4::TcpPinger;
#[cfg(feature = "udp")]
pub use crate::level4::UdpPinger;

#[cfg(feature = "dns")]
mod dns;
#[cfg(feature = "dns")]
pub use crate::dns::{DnsPinger, RecordType};

#[cfg(feature = "http")]
mod http;
#[cfg(feature = "http")]
pub use crate::http::{HttpMethod, HttpPinger};

#[cfg(feature = "ws")]
mod websocket;
#[cfg(feature = "ws")]
pub use crate::websocket::WebSocketPinger;

#[cfg(feature = "mqtt")]
mod mqtt;
#[cfg(feature = "mqtt")]
pub use crate::mqtt::{MqttPinger, MqttVersion};

#[cfg(feature = "grpc")]
mod grpc;
#[cfg(feature = "grpc")]
pub use crate::grpc::{GrpcPinger, GrpcStreamPinger};

#[cfg(feature = "hls")]
mod hls;
#[cfg(feature = "hls")]
pub use crate::hls::HlsPinger;

#[cfg(feature = "tls")]
mod tls_handshake;
#[cfg(feature = "tls")]
pub use crate::tls_handshake::TlsPinger;

#[cfg(feature = "ntp")]
mod ntp;
#[cfg(feature = "ntp")]
pub use crate::ntp::NtpPinger;

// STUN module is also a building block for TURN — gate it on either
// `stun` or `turn` so TURN compiles even when the user only enables
// the `turn` feature directly.
#[cfg(any(feature = "stun", feature = "turn"))]
mod stun;
#[cfg(feature = "stun")]
pub use crate::stun::StunPinger;

#[cfg(feature = "turn")]
mod turn;
#[cfg(feature = "turn")]
pub use crate::turn::TurnPinger;

// `BUF_SIZE` is shared by `level4` (tcp / udp) and `http`.
// `HTTP_UNCONNECT_STATUS_CODE` is http-only.
#[cfg(any(feature = "tcp", feature = "udp", feature = "http"))]
pub(crate) const BUF_SIZE: usize = 0xFF;
#[cfg(feature = "http")]
pub(crate) const HTTP_UNCONNECT_STATUS_CODE: &[&str] = &["404", "501"];

/// Resolve `url`'s host:port to a list of socket addresses for display.
/// Falls back to scheme default ports (http → 80, https → 443) when
/// the URL has no explicit port. Returns an empty Vec if DNS lookup
/// fails or the host is empty — callers (the CLI in particular)
/// treat the result as informational and let the actual pinger
/// surface the real error.
pub async fn resolve(url: &str) -> Vec<SocketAddr> {
    let uri = uri::get_uri(url);
    if uri.domain.is_empty() {
        return Vec::new();
    }
    let port = if uri.port > 0 {
        uri.port as u16
    } else {
        match uri.scheme.to_ascii_lowercase().as_str() {
            "https" | "wss" => 443,
            _ => 80,
        }
    };
    match tokio::net::lookup_host(format!("{}:{}", uri.domain, port)).await {
        Ok(iter) => iter.collect(),
        Err(_) => Vec::new(),
    }
}
