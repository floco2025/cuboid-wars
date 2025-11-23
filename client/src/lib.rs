mod client;
mod config;
mod input;
mod io;

pub use client::GameClient;
pub use common::io::MessageStream;
use common::protocol::{CLogin, ClientMessage};
pub use config::configure_client;
pub use input::user_input;
pub use io::{receive_messages, send_message};

use anyhow::{Context, Result};
use clap::Parser;
use quinn::Endpoint;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// CLI Helper
// ============================================================================

fn get_login_name() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

// ============================================================================
// CLI Arguments
// ============================================================================

#[derive(Parser, Debug)]
#[command(author, version, about = "Game client", long_about = None)]
pub struct Args {
    /// Server address to connect to
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    pub server: String,

    /// Login name
    #[arg(short, long, default_value_t = get_login_name())]
    pub name: String,
}

// ============================================================================
// Main Client Loop
// ============================================================================

pub async fn run_client() -> Result<()> {
    let args = Args::parse();

    // Create connection to the server
    println!("Connecting to server...");
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
    let client_config = configure_client()?;
    endpoint.set_default_client_config(client_config);
    let connection = endpoint
        .connect(args.server.parse()?, "localhost")?
        .await
        .context("Failed to connect to server")?;
    println!("Connected to server at {}", args.server);
    let connection = Arc::new(connection);

    // Server login
    send_message(&connection, &ClientMessage::Login(CLogin { name: args.name })).await?;

    // Create game client
    let client = Arc::new(Mutex::new(GameClient::new()));

    // Spawn task to handle server messages
    let client_clone = client.clone();
    let connection_clone = connection.clone();
    let mut recv_task = tokio::spawn(async move {
        receive_messages(connection_clone, client_clone).await;
    });

    // Spawn task to handle user input
    let mut input_task = tokio::spawn(async move { user_input(connection, client).await });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut recv_task => {
            std::process::exit(0);
        }
        _ = &mut input_task => {
            std::process::exit(0);
        }
    }
}
