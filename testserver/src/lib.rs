use std::io::{Read, Result, Write};
use std::net::{SocketAddr, TcpListener, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::thread;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig, ServerConnection, StreamOwned};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::ServerTlsConfig;
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

/// Spin up a minimal HLS server. Serves:
///   - `/playlist.m3u8` — a one-segment media playlist
///   - `/segment0.ts` — a tiny "fake TS" body (any bytes; HlsPinger
///     only checks the response is HTTP/200 with non-empty body)
///
/// Any other path returns 404. Range requests are honored on
/// `/segment0.ts` so `HlsPinger`'s `Range: bytes=0-0` probe gets a
/// proper 206 Partial Content response.
pub fn start_hls_ok<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || handle_hls(stream));
        }
    });
    Ok(bound)
}

fn handle_hls(mut stream: std::net::TcpStream) {
    let mut buf = [0u8; BUF_SIZE];
    let n = match stream.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return,
    };
    let req = String::from_utf8_lossy(&buf[..n]).to_string();
    let request_line = req.lines().next().unwrap_or("");
    let path = request_line.split_whitespace().nth(1).unwrap_or("");
    let range_hdr = req
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("range:"))
        .map(|l| l.to_string());

    let body: &[u8] = match path {
        p if p.starts_with("/playlist.m3u8") => {
            b"#EXTM3U\n\
                  #EXT-X-VERSION:3\n\
                  #EXT-X-TARGETDURATION:10\n\
                  #EXT-X-MEDIA-SEQUENCE:0\n\
                  #EXTINF:10.0,\n\
                  segment0.ts\n\
                  #EXT-X-ENDLIST\n"
        }
        p if p.starts_with("/master.m3u8") => {
            b"#EXTM3U\n\
                  #EXT-X-VERSION:3\n\
                  #EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\n\
                  playlist.m3u8\n"
        }
        p if p.starts_with("/segment0.ts") => b"FAKE_TS_SEGMENT_PAYLOAD",
        _ => {
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            return;
        }
    };

    let content_type = match path {
        p if p.contains(".m3u8") => "application/vnd.apple.mpegurl",
        _ => "video/mp2t",
    };

    if let Some(_range) = range_hdr {
        // Serve byte 0 as a 206 Partial Content. Honor only the
        // simplest "bytes=0-0" form — that's what HlsPinger sends.
        let total = body.len();
        let response = format!(
            "HTTP/1.1 206 Partial Content\r\n\
             Content-Type: {content_type}\r\n\
             Content-Range: bytes 0-0/{total}\r\n\
             Content-Length: 1\r\n\
             Connection: close\r\n\r\n"
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(&body[..1]);
    } else {
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: {content_type}\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(body);
    }
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

/// Handle returned by `start_grpcs_ok` — the address the broker is
/// bound to plus the PEM-encoded CA certificate clients need to
/// trust to talk to it.
pub struct GrpcsServer {
    pub addr: SocketAddr,
    pub ca_pem: Vec<u8>,
}

/// Spin up a plaintext gRPC server on `addr` that implements the
/// standard `grpc.health.v1.Health` service via `tonic-health`. The
/// overall ("" service) status is set to `SERVING` so a vanilla
/// `Health/Check` from `GrpcPinger` succeeds.
pub fn start_grpc_ok<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr> {
    let std_listener = TcpListener::bind(addr)?;
    std_listener.set_nonblocking(true)?;
    let bound = std_listener.local_addr()?;

    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(async move {
            let listener =
                tokio::net::TcpListener::from_std(std_listener).expect("from_std listener");
            let (mut reporter, service) = tonic_health::server::health_reporter();
            reporter
                .set_service_status("", tonic_health::ServingStatus::Serving)
                .await;
            let _ = tonic::transport::Server::builder()
                .add_service(service)
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await;
        });
    });

    Ok(bound)
}

/// TLS-wrapped variant of `start_grpc_ok` for `grpcs://` clients.
/// Returns the bound address plus the PEM bytes of the freshly
/// generated self-signed certificate, so test code can pass them to
/// `GrpcPinger::with_ca_cert(...)`.
pub fn start_grpcs_ok<A: ToSocketAddrs>(addr: A) -> Result<GrpcsServer> {
    let issued = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .map_err(|e| std::io::Error::other(format!("rcgen: {e}")))?;
    let cert_pem = issued.cert.pem().into_bytes();
    let key_pem = issued.key_pair.serialize_pem().into_bytes();

    let std_listener = TcpListener::bind(addr)?;
    std_listener.set_nonblocking(true)?;
    let bound = std_listener.local_addr()?;

    let identity = tonic::transport::Identity::from_pem(&cert_pem, &key_pem);
    let ca_pem = cert_pem.clone();

    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(async move {
            let listener =
                tokio::net::TcpListener::from_std(std_listener).expect("from_std listener");
            let (mut reporter, service) = tonic_health::server::health_reporter();
            reporter
                .set_service_status("", tonic_health::ServingStatus::Serving)
                .await;
            let tls = ServerTlsConfig::new().identity(identity);
            let _ = tonic::transport::Server::builder()
                .tls_config(tls)
                .expect("server tls_config")
                .add_service(service)
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await;
        });
    });

    Ok(GrpcsServer {
        addr: bound,
        ca_pem,
    })
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
