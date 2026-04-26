use std::io::prelude::*;
use std::io::Result;
use std::net::{TcpStream, UdpSocket};
use std::time::Duration;

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

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Pinger for TcpPinger {
    fn ping(&self) -> Result<()> {
        let mut stream = TcpStream::connect(&self.target)?;
        let mut buffer = [0; BUF_SIZE];

        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        stream.write_all(&[1])?;
        let _ = stream.read(&mut buffer)?;
        Ok(())
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

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Pinger for UdpPinger {
    fn ping(&self) -> Result<()> {
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let mut buffer = [0; BUF_SIZE];
        socket.connect(&self.target)?;

        socket.set_read_timeout(Some(self.timeout))?;
        socket.set_write_timeout(Some(self.timeout))?;

        let _ = socket.send(&[1])?;
        let _ = socket.recv_from(&mut buffer)?;
        Ok(())
    }
}

/// Function-style entry point for backward compatibility with the
/// `PingHandler` HashMap dispatch. Will be removed in a later PR once
/// the dispatcher migrates to trait objects.
pub fn tcping(target: &str) -> Result<()> {
    TcpPinger::new(target).ping()
}

/// Function-style entry point for backward compatibility with the
/// `PingHandler` HashMap dispatch. Will be removed in a later PR once
/// the dispatcher migrates to trait objects.
pub fn udping(target: &str) -> Result<()> {
    UdpPinger::new(target).ping()
}
