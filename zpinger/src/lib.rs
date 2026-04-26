use std::net::{SocketAddr, ToSocketAddrs};

#[path = "tests/test_pinger.rs"]
#[cfg(test)]
mod test_pinger;

mod http;
mod level4;
mod pinger;
mod tls;
pub mod uri;

pub use crate::http::{HttpMethod, HttpPinger};
pub use crate::level4::{TcpPinger, UdpPinger};
pub use crate::pinger::{timed, Pinger};
pub use crate::tls::default_client_config;
pub use rustls::ClientConfig;

pub(crate) const BUF_SIZE: usize = 0xFF;
pub(crate) const HTTP_UNCONNECT_STATUS_CODE: &[&str] = &["404", "501"];

/// Resolve `url`'s host:port to a list of socket addresses for display.
/// Returns an empty Vec if DNS lookup fails or the host is empty —
/// callers (the CLI in particular) treat the result as informational
/// and let the actual pinger surface the real error.
pub fn resolve(url: &str) -> Vec<SocketAddr> {
    let uri = uri::get_uri(url);
    uri.host
        .as_str()
        .to_socket_addrs()
        .map(|it| it.collect())
        .unwrap_or_default()
}
