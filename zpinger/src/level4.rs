use std::io::Result;
use std::time::Duration;

use async_trait::async_trait;

use crate::pinger::Pinger;
use crate::util::with_timeout;
use crate::BUF_SIZE;

#[cfg(any(feature = "tcp", feature = "udp"))]
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// TCP pinger — opens a TCP connection to `target`, sends one byte,
/// and waits for any response byte before closing.
#[cfg(feature = "tcp")]
pub struct TcpPinger {
    pub target: String,
    pub timeout: Duration,
}

#[cfg(feature = "tcp")]
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

#[cfg(feature = "tcp")]
#[async_trait]
impl Pinger for TcpPinger {
    async fn ping(&self) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

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
#[cfg(feature = "udp")]
pub struct UdpPinger {
    pub target: String,
    pub timeout: Duration,
}

#[cfg(feature = "udp")]
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

#[cfg(feature = "udp")]
#[async_trait]
impl Pinger for UdpPinger {
    async fn ping(&self) -> Result<()> {
        use tokio::net::UdpSocket;

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
