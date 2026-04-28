use std::io::{Read, Result, Write};
use std::net::{SocketAddr, TcpListener, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::thread;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig, ServerConnection, StreamOwned};
use tungstenite::accept;
use tungstenite::protocol::Message;

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

/// Generate a fresh self-signed cert for SAN `localhost` and return
/// the matching `ServerConfig` (for the listener side) plus a
/// `ClientConfig` whose only trust anchor is that cert (for the
/// client side). Used by every TLS-wrapped test endpoint —
/// `start_https_ok`, `start_wss_ok`, `start_mqtts_ok`.
fn make_test_tls_pair() -> Result<(Arc<ServerConfig>, Arc<ClientConfig>)> {
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

    let mut roots = RootCertStore::empty();
    roots
        .add(cert_der)
        .map_err(|e| std::io::Error::other(format!("trust anchor: {e}")))?;
    let client_config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| std::io::Error::other(format!("rustls protocol: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();

    Ok((Arc::new(server_config), Arc::new(client_config)))
}

/// Spin up an HTTPS 200-OK responder on `addr` using a freshly
/// generated self-signed cert for the SAN `localhost`. Returns the
/// bound address and a `ClientConfig` pre-loaded with the cert as a
/// trust anchor.
pub fn start_https_ok<A: ToSocketAddrs>(addr: A) -> Result<HttpsServer> {
    let (server_config, client_config) = make_test_tls_pair()?;
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

/// Spin up a minimal UDP DNS responder. Each inbound packet that
/// looks like a DNS query (≥ 12 bytes) is echoed back with the QR
/// bit flipped on, RCODE forced to 0, and the question section left
/// untouched. The answer section stays empty — just enough to make
/// `DnsPinger`'s structural validation pass without parsing actual
/// records.
pub fn start_dns_ok<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let socket = UdpSocket::bind(addr)?;
    let bound = socket.local_addr()?;
    thread::spawn(move || {
        let mut buf = [0u8; BUF_SIZE];
        loop {
            match socket.recv_from(&mut buf) {
                Ok((n, src)) if n >= 12 => {
                    buf[2] |= 0x80; // QR = 1 (response)
                    buf[3] &= 0xF0; // RCODE = 0
                    let _ = socket.send_to(&buf[..n], src);
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    Ok(bound)
}

/// Spin up a plain (`ws://`) WebSocket echo / ping server. Each
/// connection is upgraded by tungstenite, then the server replies to
/// any incoming PING with a PONG carrying the same payload.
pub fn start_ws_ok<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut ws = match accept(stream) {
                    Ok(ws) => ws,
                    Err(_) => return,
                };
                while let Ok(msg) = ws.read() {
                    match msg {
                        Message::Ping(payload) => {
                            let _ = ws.send(Message::Pong(payload));
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            });
        }
    });
    Ok(bound)
}

/// Same as `start_ws_ok` but wrapped in TLS so clients connect via
/// `wss://`. Returns the bound address plus a `ClientConfig` that
/// trusts the freshly-generated self-signed cert (and only that
/// cert) — same shape as `start_https_ok`.
pub fn start_wss_ok<A: ToSocketAddrs>(addr: A) -> Result<HttpsServer> {
    let (server_config, client_config) = make_test_tls_pair()?;
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
                    let tls = StreamOwned::new(conn, stream);
                    let mut ws = match accept(tls) {
                        Ok(ws) => ws,
                        Err(_) => return,
                    };
                    while let Ok(msg) = ws.read() {
                        match msg {
                            Message::Ping(payload) => {
                                let _ = ws.send(Message::Pong(payload));
                            }
                            Message::Close(_) => break,
                            _ => {}
                        }
                    }
                });
            }
        });
    }

    Ok(HttpsServer {
        addr: bound,
        client_config,
    })
}

/// Read a single MQTT control packet from `stream`. Returns
/// `(packet_type, body)`. Supports the same 4-byte variable-byte-int
/// remaining-length encoding the client side uses.
fn read_mqtt_packet<S: Read>(stream: &mut S) -> Result<(u8, Vec<u8>)> {
    let mut header = [0u8; 1];
    stream.read_exact(&mut header)?;
    let mut multiplier: usize = 1;
    let mut remaining: usize = 0;
    for _ in 0..4 {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte)?;
        let b = byte[0];
        remaining += (b & 0x7F) as usize * multiplier;
        if b & 0x80 == 0 {
            let mut body = vec![0u8; remaining];
            if remaining > 0 {
                stream.read_exact(&mut body)?;
            }
            return Ok((header[0], body));
        }
        multiplier = multiplier.saturating_mul(128);
    }
    Err(std::io::Error::other(
        "MQTT remaining-length varint exceeds 4 bytes",
    ))
}

/// Drive a minimal MQTT 3.1.1 broker session over an established
/// stream: accept CONNECT, return CONNACK with return-code 0, reply
/// to PINGREQ with PINGRESP, and exit cleanly on DISCONNECT or any
/// other packet type. Errors close the connection.
fn handle_mqtt_session<S: Read + Write>(mut stream: S) {
    // First packet must be CONNECT (0x10). We don't validate the body,
    // we just need to send a CONNACK back.
    let (first_type, _body) = match read_mqtt_packet(&mut stream) {
        Ok(p) => p,
        Err(_) => return,
    };
    if first_type & 0xF0 != 0x10 {
        return;
    }
    // CONNACK: type 0x20, remaining length 2, flags 0, return code 0
    if stream.write_all(&[0x20, 0x02, 0x00, 0x00]).is_err() {
        return;
    }

    loop {
        let (packet_type, _body) = match read_mqtt_packet(&mut stream) {
            Ok(p) => p,
            Err(_) => return,
        };
        match packet_type & 0xF0 {
            0xC0 => {
                // PINGREQ -> PINGRESP
                if stream.write_all(&[0xD0, 0x00]).is_err() {
                    return;
                }
            }
            0xE0 => {
                // DISCONNECT
                return;
            }
            _ => {
                // Unknown / unsupported packet type — disconnect.
                return;
            }
        }
    }
}

/// Spin up a minimal MQTT 3.1.1 broker on `addr`. Accepts the
/// CONNECT / CONNACK handshake, replies to PINGREQ with PINGRESP,
/// closes on DISCONNECT. No subscriptions, no PUBLISH support — just
/// enough to drive `MqttPinger` end-to-end.
pub fn start_mqtt_ok<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || handle_mqtt_session(stream));
        }
    });
    Ok(bound)
}

/// TLS-wrapped variant of `start_mqtt_ok` for `mqtts://` clients.
/// Returns the bound address plus a `ClientConfig` that trusts the
/// freshly-generated self-signed cert (and only that cert) — same
/// shape as `start_https_ok` / `start_wss_ok`.
pub fn start_mqtts_ok<A: ToSocketAddrs>(addr: A) -> Result<HttpsServer> {
    let (server_config, client_config) = make_test_tls_pair()?;
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
                    let tls = StreamOwned::new(conn, stream);
                    handle_mqtt_session(tls);
                });
            }
        });
    }

    Ok(HttpsServer {
        addr: bound,
        client_config,
    })
}
