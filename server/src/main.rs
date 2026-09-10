use std::net::SocketAddr;

use anyhow::Result;
use bevy::prelude::*;
use clap::Parser;
use quinn::Endpoint;
use tokio::{
    sync::mpsc::unbounded_channel,
    time::{self, Instant, MissedTickBehavior},
};

use common::config::NetworkConfig;
use server::{
    app::{NetworkOverrides, build_server_app},
    config::configure_server,
    network::{FromClientsChannel, accept_connections_task},
};

#[derive(Parser)]
#[command(author, version, about = "Cuboid Wars Server", long_about = None)]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    bind: String,

    #[arg(long)]
    map: Option<String>,

    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    server_hz: Option<u32>,

    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    update_hz: Option<u32>,

    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    snapshot_hz: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let addr: SocketAddr = args.bind.parse()?;
    let endpoint = Endpoint::server(configure_server()?, addr)?;
    println!("quic server listening on {addr}");

    let (to_server, from_clients) = unbounded_channel();
    let mut app = build_server_app(
        args.map.as_deref(),
        NetworkOverrides {
            server_hz: args.server_hz,
            update_hz: args.update_hz,
            snapshot_hz: args.snapshot_hz,
        },
        FromClientsChannel::new(from_clients),
    )?;
    tokio::spawn(accept_connections_task(endpoint, to_server));

    info!("starting ECS server loop...");

    let tick_duration = app.world().resource::<NetworkConfig>().tick_duration();
    let mut interval = time::interval(tick_duration);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut frame: u64 = 0;
    loop {
        interval.tick().await;

        let update_start = Instant::now();
        app.update();
        let update_elapsed = update_start.elapsed();

        if update_elapsed > tick_duration {
            warn!(
                "tick {} took {:.2}ms (exceeded {:.2}ms budget)",
                frame,
                update_elapsed.as_secs_f64() * 1000.0,
                tick_duration.as_secs_f64() * 1000.0
            );
        }

        frame += 1;
    }
}

#[cfg(test)]
#[path = "tests/main.rs"]
mod tests;
