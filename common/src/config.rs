use anyhow::{Context, Result};
use quinn::TransportConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::{sync::Arc, time::Duration};

// ============================================================================
// Constants
// ============================================================================

const IDLE_TIMEOUT_SECS: u64 = 5;
const KEEPALIVE_INTERVAL_SECS: u64 = 2;

// ============================================================================
// Shared Configuration
// ============================================================================

pub fn load_certs() -> Result<Vec<CertificateDer<'static>>> {
    let cert = std::fs::read("cert.pem").context("Failed to read cert.pem")?;
    rustls_pemfile::certs(&mut &cert[..])
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to parse certificates")
}

pub fn load_private_key() -> Result<PrivateKeyDer<'static>> {
    let key = std::fs::read("key.pem").context("Failed to read key.pem")?;
    rustls_pemfile::private_key(&mut &key[..])
        .context("Failed to read private key")?
        .ok_or_else(|| anyhow::anyhow!("No private key found"))
}

// Create a shared transport configuration with timeouts and keepalive
pub fn create_transport_config() -> Result<Arc<TransportConfig>> {
    let mut transport = TransportConfig::default();
    transport.max_idle_timeout(Some(
        Duration::from_secs(IDLE_TIMEOUT_SECS)
            .try_into()
            .context("Invalid idle timeout")?,
    ));
    transport.keep_alive_interval(Some(Duration::from_secs(KEEPALIVE_INTERVAL_SECS)));
    Ok(Arc::new(transport))
}

// Create a Quinn `ClientConfig` from a rustls `ClientConfig` with transport settings
pub fn create_quinn_client_config(crypto: rustls::ClientConfig) -> Result<quinn::ClientConfig> {
    let mut config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(crypto).context("Failed to create QUIC client config")?,
    ));
    let transport = create_transport_config()?;
    config.transport_config(transport);
    Ok(config)
}

// Create a Quinn `ServerConfig` from a rustls `ServerConfig` with transport settings
pub fn create_quinn_server_config(crypto: rustls::ServerConfig) -> Result<quinn::ServerConfig> {
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(crypto).context("Failed to create QUIC server config")?,
    ));
    let transport = create_transport_config()?;
    config.transport_config(transport);
    Ok(config)
}
