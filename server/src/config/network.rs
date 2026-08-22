use anyhow::{Context, Result};
use quinn::ServerConfig;

use common::{
    config::{create_quinn_server_config, load_certs, load_private_key},
    network::ALPN_PROTOCOL,
};

pub fn configure_server() -> Result<ServerConfig> {
    let certs = load_certs()?;
    let private_key = load_private_key()?;

    let mut crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, private_key)
        .context("Failed to configure TLS")?;
    crypto.alpn_protocols = vec![ALPN_PROTOCOL.to_vec()];

    create_quinn_server_config(crypto)
}
