use std::sync::{Arc, OnceLock};

use rustls::ClientConfig;

/// Lazily-built default `ClientConfig` for plain HTTPS — uses
/// Mozilla's bundled root CAs from `webpki-roots`. Construction is
/// deferred so callers that never touch HTTPS pay nothing.
static DEFAULT_CLIENT_CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();

pub fn default_client_config() -> Arc<ClientConfig> {
    DEFAULT_CLIENT_CONFIG
        .get_or_init(|| {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .expect("ring provider supports default protocol versions")
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}
