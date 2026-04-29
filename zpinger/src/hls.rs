//! HLS pinger — measures the player-visible startup latency:
//!   1. GET the M3U8 (master or media playlist)
//!   2. if it's a master playlist, follow the first `EXT-X-STREAM-INF`
//!      variant and fetch its media playlist too
//!   3. fetch the first `.ts` / `.m4s` segment with `Range: bytes=0-0`
//!      so we measure "first byte of segment" without paying the full
//!      segment download
//!
//! All three GETs are folded into the single `time=` the trait reports.
//! `https://` reuses the rustls + webpki-roots layer from PR 8;
//! `with_tls_config` overrides for self-signed test endpoints.

use std::io::{self, Result};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::pinger::Pinger;
use crate::tls::default_client_config;
use crate::uri::{get_uri, URI};
use crate::util::with_timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct HlsPinger {
    pub url: String,
    pub timeout: Duration,
    tls_config: Option<Arc<ClientConfig>>,
}

impl HlsPinger {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            timeout: DEFAULT_TIMEOUT,
            tls_config: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_tls_config(mut self, config: Arc<ClientConfig>) -> Self {
        self.tls_config = Some(config);
        self
    }
}

#[async_trait]
impl Pinger for HlsPinger {
    async fn ping(&self) -> Result<()> {
        let url = self.url.clone();
        let timeout = self.timeout;
        let tls_config = self.tls_config.clone();
        with_timeout(timeout, async move {
            // 1. Fetch the playlist the user gave us.
            let body = http_get(&url, &tls_config).await?;
            let playlist = std::str::from_utf8(&body).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("playlist is not valid UTF-8: {e}"),
                )
            })?;
            if !playlist.lines().next().unwrap_or("").starts_with("#EXTM3U") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "response is not an M3U8 playlist (missing #EXTM3U header)",
                ));
            }

            // 2. If master, follow the first variant to get a media playlist.
            let (media_url, media_text) = if let Some(variant) = first_variant_url(playlist) {
                let resolved = resolve_relative(&url, variant)?;
                let variant_body = http_get(&resolved, &tls_config).await?;
                let variant_text = std::str::from_utf8(&variant_body)
                    .map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("variant playlist is not valid UTF-8: {e}"),
                        )
                    })?
                    .to_string();
                (resolved, variant_text)
            } else {
                (url.clone(), playlist.to_string())
            };

            // 3. Fetch the first segment, range-limited to the first byte.
            let segment = first_segment_url(&media_text).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "no segments in media playlist")
            })?;
            let segment_url = resolve_relative(&media_url, segment)?;
            let _ = http_get_with_range(&segment_url, &tls_config, Some("bytes=0-0")).await?;

            Ok(())
        })
        .await
    }
}

// -- M3U8 parsing -----------------------------------------------------

/// Find the first variant URL in a master playlist (line following an
/// `#EXT-X-STREAM-INF:` tag). Returns None when this is a media
/// playlist (no `STREAM-INF` lines).
fn first_variant_url(playlist: &str) -> Option<&str> {
    let mut iter = playlist.lines();
    while let Some(line) = iter.next() {
        if line.trim_start().starts_with("#EXT-X-STREAM-INF") {
            // The next non-comment, non-empty line is the variant URL.
            for next in iter.by_ref() {
                let next = next.trim();
                if !next.is_empty() && !next.starts_with('#') {
                    return Some(next);
                }
            }
        }
    }
    None
}

/// Find the first segment URL in a media playlist (line following an
/// `#EXTINF:` tag).
fn first_segment_url(playlist: &str) -> Option<&str> {
    let mut iter = playlist.lines();
    while let Some(line) = iter.next() {
        if line.trim_start().starts_with("#EXTINF") {
            for next in iter.by_ref() {
                let next = next.trim();
                if !next.is_empty() && !next.starts_with('#') {
                    return Some(next);
                }
            }
        }
    }
    None
}

/// Resolve a possibly-relative HLS URL against a base URL. Handles the
/// three common shapes a playlist can carry:
///
///   - absolute: `https://cdn.example.com/seg0.ts` (used as-is)
///   - root-relative: `/path/seg0.ts` (joins base scheme + host)
///   - same-directory: `seg0.ts` (joins base directory)
fn resolve_relative(base: &str, reference: &str) -> Result<String> {
    if reference.starts_with("http://") || reference.starts_with("https://") {
        return Ok(reference.to_string());
    }
    let uri = get_uri(base);
    let scheme = if uri.scheme.is_empty() {
        "http"
    } else {
        uri.scheme.as_str()
    };
    if uri.host.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot resolve relative URL: base has no host",
        ));
    }
    if let Some(rest) = reference.strip_prefix('/') {
        return Ok(format!("{scheme}://{}/{rest}", uri.host));
    }
    let dir = uri
        .path
        .rsplit_once('/')
        .map(|(d, _)| d.to_string())
        .unwrap_or_default();
    if dir.is_empty() {
        Ok(format!("{scheme}://{}/{reference}", uri.host))
    } else {
        Ok(format!("{scheme}://{}{}/{reference}", uri.host, dir))
    }
}

// -- minimal HTTP fetcher ---------------------------------------------

async fn http_get(url: &str, tls_config: &Option<Arc<ClientConfig>>) -> Result<Vec<u8>> {
    http_get_with_range(url, tls_config, None).await
}

async fn http_get_with_range(
    url: &str,
    tls_config: &Option<Arc<ClientConfig>>,
    range: Option<&str>,
) -> Result<Vec<u8>> {
    let uri = get_uri(url);
    let scheme = uri.scheme.to_ascii_lowercase();
    match scheme.as_str() {
        "" | "http" => fetch_plain(&uri, range).await,
        "https" => fetch_tls(&uri, tls_config, range).await,
        other => Err(io::Error::other(format!(
            "scheme '{other}' is not supported by HlsPinger (use http:// or https://)"
        ))),
    }
}

async fn fetch_plain(uri: &URI, range: Option<&str>) -> Result<Vec<u8>> {
    let endpoint = endpoint_for(uri, 80)?;
    let request = build_get(uri, &endpoint, range);
    let mut stream = TcpStream::connect(&endpoint).await?;
    fetch_response_body(&mut stream, &request).await
}

async fn fetch_tls(
    uri: &URI,
    tls_config: &Option<Arc<ClientConfig>>,
    range: Option<&str>,
) -> Result<Vec<u8>> {
    let endpoint = endpoint_for(uri, 443)?;
    let server_name = ServerName::try_from(uri.domain.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let config = tls_config.clone().unwrap_or_else(default_client_config);
    let request = build_get(uri, &endpoint, range);
    let tcp = TcpStream::connect(&endpoint).await?;
    let connector = TlsConnector::from(config);
    let mut stream = connector.connect(server_name, tcp).await?;
    fetch_response_body(&mut stream, &request).await
}

fn endpoint_for(uri: &URI, default_port: u16) -> Result<String> {
    if uri.domain.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing host in URL",
        ));
    }
    let port = if uri.port > 0 {
        uri.port as u16
    } else {
        default_port
    };
    Ok(format!("{}:{}", uri.domain, port))
}

fn build_get(uri: &URI, host_header: &str, range: Option<&str>) -> String {
    let path = if uri.path.is_empty() { "/" } else { &uri.path };
    let mut req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host_header}\r\n\
         User-Agent: Knock Knock\r\n\
         Accept: */*\r\n\
         Connection: close\r\n",
    );
    if let Some(r) = range {
        req.push_str(&format!("Range: {r}\r\n"));
    }
    req.push_str("\r\n");
    req
}

async fn fetch_response_body<S>(stream: &mut S, request: &str) -> Result<Vec<u8>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream.write_all(request.as_bytes()).await?;

    // Read until EOF. Servers cooperate because we sent
    // `Connection: close`. For Range requests, the body is tiny.
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }

    let split = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| io::Error::other("response missing header/body separator"))?;

    let header = std::str::from_utf8(&buf[..split])
        .map_err(|_| io::Error::other("response headers are not valid UTF-8"))?;
    let status_line = header.lines().next().unwrap_or("");
    if !status_line.starts_with("HTTP/") {
        return Err(io::Error::other(
            "response is not HTTP/1.x (wrong port or protocol?)",
        ));
    }
    // Accept 200 OK and 206 Partial Content (Range requests).
    if !status_line.contains(" 200 ") && !status_line.contains(" 206 ") {
        return Err(io::Error::other(format!(
            "unexpected HTTP status: {status_line}"
        )));
    }

    Ok(buf[split + 4..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_variant_url_picks_line_after_stream_inf() {
        let m = "#EXTM3U\n#EXT-X-VERSION:3\n\
                 #EXT-X-STREAM-INF:BANDWIDTH=800000\n\
                 low.m3u8\n\
                 #EXT-X-STREAM-INF:BANDWIDTH=2400000\n\
                 high.m3u8\n";
        assert_eq!(first_variant_url(m), Some("low.m3u8"));
    }

    #[test]
    fn first_variant_url_returns_none_for_media_playlist() {
        let m = "#EXTM3U\n#EXTINF:10.0,\nseg0.ts\n";
        assert!(first_variant_url(m).is_none());
    }

    #[test]
    fn first_segment_url_picks_line_after_extinf() {
        let m = "#EXTM3U\n#EXT-X-TARGETDURATION:10\n\
                 #EXTINF:10.0,\nseg0.ts\n\
                 #EXTINF:10.0,\nseg1.ts\n#EXT-X-ENDLIST\n";
        assert_eq!(first_segment_url(m), Some("seg0.ts"));
    }

    #[test]
    fn first_segment_url_handles_absolute_segment_url() {
        let m = "#EXTM3U\n#EXTINF:5.0,\nhttps://cdn.example.com/seg0.ts\n";
        assert_eq!(
            first_segment_url(m),
            Some("https://cdn.example.com/seg0.ts")
        );
    }

    #[test]
    fn resolve_relative_passes_absolute_through() {
        assert_eq!(
            resolve_relative(
                "https://cdn.example.com/path/playlist.m3u8",
                "https://other.example.com/seg.ts"
            )
            .unwrap(),
            "https://other.example.com/seg.ts"
        );
    }

    #[test]
    fn resolve_relative_root_relative() {
        assert_eq!(
            resolve_relative(
                "https://cdn.example.com:8443/path/playlist.m3u8",
                "/cdn/seg.ts"
            )
            .unwrap(),
            "https://cdn.example.com:8443/cdn/seg.ts"
        );
    }

    #[test]
    fn resolve_relative_same_directory() {
        assert_eq!(
            resolve_relative("https://cdn.example.com:8443/path/playlist.m3u8", "seg.ts").unwrap(),
            "https://cdn.example.com:8443/path/seg.ts"
        );
    }
}
