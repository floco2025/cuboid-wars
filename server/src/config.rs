use anyhow::{Context, Result};
use quinn::ServerConfig;
use tracing_subscriber::EnvFilter;

// ============================================================================
// Tracing Initialization
// ============================================================================

/// Initialize tracing based on `RUST_LOG` environment variable
///
/// - Levels: error, warn, info, debug, trace (least to most verbose)
/// - Examples: `RUST_LOG=debug`, `RUST_LOG=chat_async=trace`
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(true)
        .with_writer(std::io::stderr)
        .init();
}

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
    crypto.alpn_protocols = vec![b"chat".to_vec()];

    common::config::create_quinn_server_config(crypto)
}
