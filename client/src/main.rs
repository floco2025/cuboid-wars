use anyhow::{Context, Result};
use clap::Parser;
use quinn::Endpoint;
use tokio::{runtime::Runtime, sync::mpsc::unbounded_channel, time::Duration};

use client::{
    app::{ClientAppOptions, build_client_app},
    network::{ClientToServerChannel, Impairment, ServerToClientChannel, configure_client, network_io_task},
};
use common::protocol::*;

#[derive(Parser, Debug)]
#[command(author, version, about = "Cuboid Wars", long_about = None)]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    server: String,

    #[arg(short, long)]
    name: Option<String>,

    #[arg(long, default_value = "0")]
    lag_ms: u64,

    // Fraction of unreliable messages to discard, sent and received.
    #[arg(long, default_value = "0")]
    drop: f32,

    #[arg(long)]
    window_x: Option<i32>,

    #[arg(long)]
    window_y: Option<i32>,

    #[arg(long, default_value = "1200")]
    window_width: u32,

    #[arg(long, default_value = "800")]
    window_height: u32,

    #[arg(long)]
    volume: Option<f32>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let (to_client, from_server) = unbounded_channel();
    let (to_server, from_client) = unbounded_channel();
    let player_name = args.name.unwrap_or_else(|| {
        let full_name = whoami::realname().unwrap_or_default();
        full_name.split_whitespace().next().unwrap_or_default().to_string()
    });
    let runtime = Runtime::new()?;
    let connection = connect_to_server(&runtime, args.server.as_str())?;
    if !(0.0..=1.0).contains(&args.drop) {
        anyhow::bail!("--drop must be within 0..=1, got {}", args.drop);
    }
    let impairment = Impairment {
        lag: Duration::from_millis(args.lag_ms),
        drop_probability: args.drop,
    };
    runtime.spawn(network_io_task(connection, to_client, from_client, impairment));
    to_server
        .send(client::network::ClientToServer::Send(ClientMessage::Login(CLogin {
            name: player_name,
        })))
        .context("network task stopped before login")?;
    let mut from_server = from_server;
    let bootstrap = wait_for_init(&runtime, &mut from_server)?;
    let mut app = build_client_app(
        ClientAppOptions {
            window_x: args.window_x,
            window_y: args.window_y,
            window_width: args.window_width,
            window_height: args.window_height,
            volume: args.volume,
        },
        ClientToServerChannel::new(to_server),
        ServerToClientChannel::new(from_server),
        bootstrap,
    )?;
    // Winit's macOS event loop can leave SIGINT queued without waking the
    // application, so service it from Tokio and use the conventional exit code.
    runtime.spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            std::process::exit(130);
        }
    });

    app.run();
    // Tokio and winit do not always finish tearing down after AppExit on macOS.
    std::process::exit(0);
}

fn connect_to_server(runtime: &Runtime, server_addr: &str) -> Result<quinn::Connection> {
    runtime.block_on(async {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
        let client_config = configure_client()?;
        endpoint.set_default_client_config(client_config);
        endpoint
            .connect(server_addr.parse()?, "localhost")?
            .await
            .context("failed to connect to server")
    })
}

// Anything that lands before `SInit` is a snapshot or an unreliable message,
// which the protocol lets us drop; the reliable lane guarantees `SInit`
// comes first on it.
fn wait_for_init(
    runtime: &Runtime,
    from_server: &mut tokio::sync::mpsc::UnboundedReceiver<client::network::ServerToClient>,
) -> Result<SInit> {
    runtime.block_on(async {
        loop {
            match from_server.recv().await {
                Some(client::network::ServerToClient::Message(ServerMessage::Init(message))) => return Ok(message),
                Some(client::network::ServerToClient::Message(_)) => {}
                Some(client::network::ServerToClient::Disconnected) | None => {
                    anyhow::bail!("server disconnected before SInit")
                }
            }
        }
    })
}
