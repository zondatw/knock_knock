//! Shared utilities used by every protocol module that drives a
//! socket directly (everything except `grpc`, which lets tonic handle
//! its own timeouts). Gated on the union of the protocol features
//! that actually call `with_timeout` so the `--no-default-features`
//! build doesn't warn about dead code.

#[cfg(any(
    feature = "tcp",
    feature = "udp",
    feature = "dns",
    feature = "http",
    feature = "ws",
    feature = "mqtt",
    feature = "hls",
))]
use std::io::{self, Result};
#[cfg(any(
    feature = "tcp",
    feature = "udp",
    feature = "dns",
    feature = "http",
    feature = "ws",
    feature = "mqtt",
    feature = "hls",
))]
use std::time::Duration;
#[cfg(any(
    feature = "tcp",
    feature = "udp",
    feature = "dns",
    feature = "http",
    feature = "ws",
    feature = "mqtt",
    feature = "hls",
))]
use tokio::time::timeout;

/// Wrap an async operation in a deadline. tokio sockets don't expose
/// per-read / per-write timeouts; we apply a single overall timeout
/// instead, which fits the "ping" use case where total time is short
/// and any individual op stalling means the whole ping has stalled.
#[cfg(any(
    feature = "tcp",
    feature = "udp",
    feature = "dns",
    feature = "http",
    feature = "ws",
    feature = "mqtt",
    feature = "hls",
))]
pub(crate) async fn with_timeout<F>(d: Duration, fut: F) -> Result<()>
where
    F: std::future::Future<Output = Result<()>>,
{
    match timeout(d, fut).await {
        Ok(inner) => inner,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "operation timed out",
        )),
    }
}
