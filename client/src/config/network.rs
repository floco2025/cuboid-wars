use anyhow::{Context, Result};
use quinn::ClientConfig;

use common::config::{create_quinn_client_config, load_certs};

// ============================================================================
// Connection Configuration
// ============================================================================

pub fn configure_client() -> Result<ClientConfig> {
    let certs = load_certs()?;

    let mut roots = rustls::RootCertStore::empty();
    for cert in certs {
        roots.add(cert).context("Failed to add certificate to root store")?;
    }

    let mut crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    crypto.alpn_protocols = vec![b"game".to_vec()];

    create_quinn_client_config(crypto)
}
