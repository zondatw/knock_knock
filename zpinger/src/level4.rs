use std::io::{self, Result};
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::time::timeout;

use crate::pinger::Pinger;
use crate::BUF_SIZE;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// TCP pinger — opens a TCP connection to `target`, sends one byte,
/// and waits for any response byte before closing.
pub struct TcpPinger {
    pub target: String,
    pub timeout: Duration,
}

impl TcpPinger {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }
}

#[async_trait]
impl Pinger for TcpPinger {
    async fn ping(&self) -> Result<()> {
        with_timeout(self.timeout, async {
            let mut stream = TcpStream::connect(&self.target).await?;
            stream.write_all(&[1]).await?;
            let mut buf = [0u8; BUF_SIZE];
            let _ = stream.read(&mut buf).await?;
            Ok(())
        })
        .await
    }
}

/// UDP pinger — sends one datagram to `target` from an ephemeral local
/// socket and waits for a datagram in reply.
pub struct UdpPinger {
    pub target: String,
    pub timeout: Duration,
}

impl UdpPinger {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }
}

#[async_trait]
impl Pinger for UdpPinger {
    async fn ping(&self) -> Result<()> {
        with_timeout(self.timeout, async {
            let socket = UdpSocket::bind("0.0.0.0:0").await?;
            socket.connect(&self.target).await?;
            socket.send(&[1]).await?;
            let mut buf = [0u8; BUF_SIZE];
            let _ = socket.recv(&mut buf).await?;
            Ok(())
        })
        .await
    }
}

/// Wrap an async operation in a deadline. tokio sockets don't expose
/// per-read / per-write timeouts; we apply a single overall timeout
/// instead, which fits the "ping" use case where total time is short
/// and any individual op stalling means the whole ping has stalled.
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
