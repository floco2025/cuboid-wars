use anyhow::Result;
use clap::Parser;
use quinn::Endpoint;
use server::config::{configure_server, init_tracing};
use server::server::GameServer;
use server::net::ClientToServer;
use std::net::SocketAddr;
use tokio::sync::mpsc::unbounded_channel;
use tracing::info;

// ============================================================================
// CLI Argument Parsing
// ============================================================================

#[derive(Parser)]
#[command(author, version, about = "Game server", long_about = None)]
struct Args {
    /// Address to bind server to
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    bind: String,
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    init_tracing();

    let args = Args::parse();

    let addr: SocketAddr = args.bind.parse()?;
    let server_config = configure_server()?;
    let endpoint = Endpoint::server(server_config, addr)?;
    info!("QUIC server listening on {}", addr);

    let mut server = GameServer::new();

    // Channel for sending from all per client network IO tasks to the server
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
