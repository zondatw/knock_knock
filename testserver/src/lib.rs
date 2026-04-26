use std::io::{Read, Result, Write};
use std::net::{SocketAddr, TcpListener, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::thread;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig, ServerConnection, StreamOwned};

const BUF_SIZE: usize = 1024;

pub fn start_tcp_echo<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut s = stream;
                let mut buf = [0u8; BUF_SIZE];
                if let Ok(n) = s.read(&mut buf) {
                    let _ = s.write_all(&buf[..n]);
                }
            });
        }
    });
    Ok(bound)
}

pub fn start_udp_echo<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let socket = UdpSocket::bind(addr)?;
    let bound = socket.local_addr()?;
    thread::spawn(move || {
        let mut buf = [0u8; BUF_SIZE];
        loop {
            if let Ok((n, src)) = socket.recv_from(&mut buf) {
                let _ = socket.send_to(&buf[..n], src);
            }
        }
    });
    Ok(bound)
}

pub fn start_http_ok<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut s = stream;
                let mut buf = [0u8; BUF_SIZE];
                let _ = s.read(&mut buf);
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            });
        }
    });
    Ok(bound)
}

/// Handle returned by `start_https_ok` — exposes the bound address
/// plus a `ClientConfig` whose only trust anchor is the self-signed
/// cert this server uses, so test code can speak HTTPS to the server
/// without pulling in the system trust store.
pub struct HttpsServer {
    pub addr: SocketAddr,
    pub client_config: Arc<ClientConfig>,
}

/// Spin up an HTTPS 200-OK responder on `addr` using a freshly
/// generated self-signed cert for the SAN `localhost`. Returns the
/// bound address and a `ClientConfig` pre-loaded with the cert as a
/// trust anchor.
pub fn start_https_ok<A: ToSocketAddrs>(addr: A) -> Result<HttpsServer> {
    let issued = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .map_err(|e| std::io::Error::other(format!("rcgen: {e}")))?;
    let cert_der = CertificateDer::from(issued.cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(issued.key_pair.serialize_der()));

    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let server_config = ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| std::io::Error::other(format!("rustls protocol: {e}")))?
        .with_no_client_auth()
        .with_single_cert(vec![cert_der.clone()], key_der)
        .map_err(|e| std::io::Error::other(format!("rustls server cert: {e}")))?;
    let server_config = Arc::new(server_config);

    let mut roots = RootCertStore::empty();
    roots
        .add(cert_der)
        .map_err(|e| std::io::Error::other(format!("trust anchor: {e}")))?;
    let client_config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| std::io::Error::other(format!("rustls protocol: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let client_config = Arc::new(client_config);

    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;

    {
        let server_config = Arc::clone(&server_config);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let cfg = Arc::clone(&server_config);
                thread::spawn(move || {
                    let conn = match ServerConnection::new(cfg) {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    let mut tls = StreamOwned::new(conn, stream);
                    let mut buf = [0u8; BUF_SIZE];
                    let _ = tls.read(&mut buf);
                    let _ = tls.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                });
            }
        });
    }

    Ok(HttpsServer {
        addr: bound,
        client_config,
    })
}
