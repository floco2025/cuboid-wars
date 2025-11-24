use anyhow::{Context, Result};
use quinn::ServerConfig;

// ============================================================================
// Connection Configuration
// ============================================================================

pub fn configure_server() -> Result<ServerConfig> {
    let certs = common::config::load_certs()?;
    let private_key = common::config::load_private_key()?;

    let mut crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, private_key)
        .context("Failed to configure TLS")?;
    crypto.alpn_protocols = vec![b"game".to_vec()];

    common::config::create_quinn_server_config(crypto)
}
