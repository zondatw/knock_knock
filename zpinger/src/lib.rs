use std::net::{SocketAddr, ToSocketAddrs};

#[path = "tests/test_pinger.rs"]
#[cfg(test)]
mod test_pinger;

mod dns;
mod http;
mod level4;
mod mqtt;
mod pinger;
mod tls;
pub mod uri;
mod websocket;

pub use crate::dns::{DnsPinger, RecordType};
pub use crate::http::{HttpMethod, HttpPinger};
pub use crate::level4::{TcpPinger, UdpPinger};
pub use crate::mqtt::{MqttPinger, MqttVersion};
pub use crate::pinger::{timed, Pinger};
pub use crate::tls::default_client_config;
pub use crate::websocket::WebSocketPinger;
pub use rustls::ClientConfig;

pub(crate) const BUF_SIZE: usize = 0xFF;
pub(crate) const HTTP_UNCONNECT_STATUS_CODE: &[&str] = &["404", "501"];

/// Resolve `url`'s host:port to a list of socket addresses for display.
/// Falls back to scheme default ports (http → 80, https → 443) when
/// the URL has no explicit port. Returns an empty Vec if DNS lookup
/// fails or the host is empty — callers (the CLI in particular)
/// treat the result as informational and let the actual pinger
/// surface the real error.
pub fn resolve(url: &str) -> Vec<SocketAddr> {
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
    format!("{}:{}", uri.domain, port)
        .to_socket_addrs()
        .map(|it| it.collect())
        .unwrap_or_default()
}
