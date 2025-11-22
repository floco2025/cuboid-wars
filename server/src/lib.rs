mod config;
mod io;
mod server;

pub use config::{configure_server, init_tracing};
pub use io::ClientToServer;
pub use server::ChatServer;

use anyhow::Result;
use clap::Parser;
use quinn::Endpoint;
use std::net::SocketAddr;
use tokio::sync::mpsc::unbounded_channel;
use tracing::info;

// ============================================================================
// CLI Argument Parsing
// ============================================================================

#[derive(Parser)]
#[command(author, version, about = "Chat server", long_about = None)]
pub struct Args {
    /// Address to bind server to
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    pub bind: String,
}

// ============================================================================
// Main Server Loop
// ============================================================================

pub async fn run_server() -> Result<()> {
    init_tracing();

    let args = Args::parse();

    let addr: SocketAddr = args.bind.parse()?;
    let server_config = configure_server()?;
    let endpoint = Endpoint::server(server_config, addr)?;
    info!("QUIC server listening on {}", addr);

    let mut server = ChatServer::new();

    // Channel for client tasks to send messages to server
    let (to_server, mut from_clients) = unbounded_channel::<(u32, ClientToServer)>();

    loop {
        tokio::select! {
            // Accept new connections
            Some(incoming) = endpoint.accept() => {
                server.accept_client(to_server.clone(), incoming).await;
            }

            // Process messages from clients
            Some((id, msg)) = from_clients.recv() => {
                match msg {
                    ClientToServer::Message(line) => {
                        server.process_client_data(id, line);
                    }
                    ClientToServer::Disconnected => {
                        server.disconnect_client(id);
                    }
                }
            }
        }
    }
}
