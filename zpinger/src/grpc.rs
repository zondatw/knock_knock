//! gRPC pinger — calls the standard
//! [gRPC Health Checking Protocol](https://github.com/grpc/grpc/blob/master/doc/health-checking.md)
//! `grpc.health.v1.Health/Check` unary RPC and validates the response
//! is `SERVING`.
//!
//! Plugs into the existing async `Pinger` trait via tonic. Like the
//! other pingers, supports plain (`grpc://` or `http://`) and TLS
//! (`grpcs://` or `https://`) endpoints. `with_ca_cert` injects a
//! caller-supplied PEM trust anchor for self-signed test endpoints;
//! the production default trusts the Mozilla root CA bundle via
//! tonic's `tls-webpki-roots` feature.

use std::io::{self, Result};
use std::time::Duration;

use async_trait::async_trait;
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint};
use tonic_health::pb::health_check_response::ServingStatus;
use tonic_health::pb::health_client::HealthClient;
use tonic_health::pb::HealthCheckRequest;

use futures_util::StreamExt;

use crate::pinger::Pinger;
use crate::uri::get_uri;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct GrpcPinger {
    pub endpoint: String,
    pub service: String,
    pub timeout: Duration,
    ca_cert_pem: Option<Vec<u8>>,
}

impl GrpcPinger {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            service: String::new(),
            timeout: DEFAULT_TIMEOUT,
            ca_cert_pem: None,
        }
    }

    /// gRPC service name passed in `HealthCheckRequest.service`. The
    /// empty string asks for the server's overall health, which is
    /// the right answer for a generic ping.
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = service.into();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// PEM-encoded CA certificate that signs the server's TLS cert.
    /// Required for testing against self-signed endpoints; production
    /// `grpcs://` endpoints should be signed by a public CA covered
    /// by webpki-roots and need no override.
    pub fn with_ca_cert(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.ca_cert_pem = Some(pem.into());
        self
    }
}

#[async_trait]
impl Pinger for GrpcPinger {
    async fn ping(&self) -> Result<()> {
        let url = normalize_endpoint(&self.endpoint)?;
        let uri = get_uri(&self.endpoint);
        let domain = uri.domain.clone();

        let mut endpoint = Endpoint::from_shared(url)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?
            .timeout(self.timeout)
            .connect_timeout(self.timeout);

        let scheme = uri.scheme.to_ascii_lowercase();
        let is_tls = scheme == "grpcs" || scheme == "https";

        if is_tls {
            let mut tls = ClientTlsConfig::new().with_webpki_roots();
            if let Some(pem) = &self.ca_cert_pem {
                tls = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(pem.clone()));
            }
            if !domain.is_empty() {
                tls = tls.domain_name(domain);
            }
            endpoint = endpoint
                .tls_config(tls)
                .map_err(|e| io::Error::other(format!("tonic tls_config: {e}")))?;
        }

        let channel = endpoint
            .connect()
            .await
            .map_err(|e| io::Error::other(format!("connect: {e}")))?;
        let mut client = HealthClient::new(channel);
        let req = HealthCheckRequest {
            service: self.service.clone(),
        };
        let resp = client
            .check(req)
            .await
            .map_err(|status| io::Error::other(format!("Health/Check: {status}")))?
            .into_inner();
        if resp.status != ServingStatus::Serving as i32 {
            return Err(io::Error::other(format!(
                "gRPC server returned status {} (expected SERVING=1)",
                resp.status
            )));
        }
        Ok(())
    }
}

/// gRPC server-streaming pinger — calls the standard
/// `grpc.health.v1.Health/Watch` RPC and waits for the broker's first
/// `HealthCheckResponse`. Per the spec, the server **must** send the
/// current status immediately on subscribe, so this measures the
/// real "open server stream → first message" RTT, not just connection
/// setup.
///
/// Same builder shape as `GrpcPinger` (endpoint / service / timeout /
/// CA cert override). Use this for endpoints that expose Watch in
/// addition to (or instead of) Check.
pub struct GrpcStreamPinger {
    pub endpoint: String,
    pub service: String,
    pub timeout: Duration,
    ca_cert_pem: Option<Vec<u8>>,
}

impl GrpcStreamPinger {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            service: String::new(),
            timeout: DEFAULT_TIMEOUT,
            ca_cert_pem: None,
        }
    }

    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = service.into();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_ca_cert(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.ca_cert_pem = Some(pem.into());
        self
    }
}

#[async_trait]
impl Pinger for GrpcStreamPinger {
    async fn ping(&self) -> Result<()> {
        let url = normalize_endpoint(&self.endpoint)?;
        let uri = get_uri(&self.endpoint);
        let domain = uri.domain.clone();

        let mut endpoint = Endpoint::from_shared(url)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?
            .timeout(self.timeout)
            .connect_timeout(self.timeout);

        let scheme = uri.scheme.to_ascii_lowercase();
        let is_tls = scheme == "grpcs" || scheme == "https";
        if is_tls {
            let mut tls = ClientTlsConfig::new().with_webpki_roots();
            if let Some(pem) = &self.ca_cert_pem {
                tls = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(pem.clone()));
            }
            if !domain.is_empty() {
                tls = tls.domain_name(domain);
            }
            endpoint = endpoint
                .tls_config(tls)
                .map_err(|e| io::Error::other(format!("tonic tls_config: {e}")))?;
        }

        let channel = endpoint
            .connect()
            .await
            .map_err(|e| io::Error::other(format!("connect: {e}")))?;
        let mut client = HealthClient::new(channel);
        let req = HealthCheckRequest {
            service: self.service.clone(),
        };
        let response = client
            .watch(req)
            .await
            .map_err(|status| io::Error::other(format!("Health/Watch: {status}")))?;
        let mut stream = response.into_inner();
        let first = stream
            .next()
            .await
            .ok_or_else(|| io::Error::other("Watch stream closed before first message"))?
            .map_err(|status| io::Error::other(format!("Watch stream error: {status}")))?;
        if first.status != ServingStatus::Serving as i32 {
            return Err(io::Error::other(format!(
                "first watched status was {} (expected SERVING=1)",
                first.status
            )));
        }
        Ok(())
    }
}

/// Translate `grpc://` / `grpcs://` schemes (used by tools like
/// `grpcurl`) to the `http://` / `https://` form tonic's `Endpoint`
/// expects. Everything else is passed through untouched, which is
/// what tonic wants for `http://` / `https://` endpoints.
fn normalize_endpoint(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "empty gRPC endpoint",
        ));
    }
    if let Some(rest) = trimmed.strip_prefix("grpcs://") {
        Ok(format!("https://{rest}"))
    } else if let Some(rest) = trimmed.strip_prefix("grpc://") {
        Ok(format!("http://{rest}"))
    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed.to_string())
    } else {
        // Default to plaintext h2c when the user gave just a host:port.
        Ok(format!("http://{trimmed}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_endpoint_passes_http_through() {
        assert_eq!(
            normalize_endpoint("http://localhost:50051").unwrap(),
            "http://localhost:50051"
        );
    }

    #[test]
    fn normalize_endpoint_passes_https_through() {
        assert_eq!(
            normalize_endpoint("https://broker:443").unwrap(),
            "https://broker:443"
        );
    }

    #[test]
    fn normalize_endpoint_rewrites_grpc_to_http() {
        assert_eq!(
            normalize_endpoint("grpc://localhost:50051").unwrap(),
            "http://localhost:50051"
        );
    }

    #[test]
    fn normalize_endpoint_rewrites_grpcs_to_https() {
        assert_eq!(
            normalize_endpoint("grpcs://broker:443").unwrap(),
            "https://broker:443"
        );
    }

    #[test]
    fn normalize_endpoint_defaults_schemeless_to_http() {
        assert_eq!(
            normalize_endpoint("localhost:50051").unwrap(),
            "http://localhost:50051"
        );
    }

    #[test]
    fn normalize_endpoint_rejects_empty() {
        assert!(normalize_endpoint("").is_err());
        assert!(normalize_endpoint("   ").is_err());
    }
}
