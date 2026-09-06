use std::net::SocketAddr;

use anyhow::Result;
use bevy::prelude::*;
use clap::Parser;
use quinn::Endpoint;
use tokio::{
    sync::mpsc::unbounded_channel,
    time::{self, Instant, MissedTickBehavior},
};

use common::constants::TICK_DURATION;
use server::{
    app::build_server_app,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let addr: SocketAddr = args.bind.parse()?;
    let endpoint = Endpoint::server(configure_server()?, addr)?;
    println!("quic server listening on {addr}");

    let (to_server, from_clients) = unbounded_channel();
    let mut app = build_server_app(args.map.as_deref(), FromClientsChannel::new(from_clients))?;
    tokio::spawn(accept_connections_task(endpoint, to_server));

    info!("starting ECS server loop...");

    let mut interval = time::interval(TICK_DURATION);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut frame: u64 = 0;
    loop {
        interval.tick().await;

        let update_start = Instant::now();
        app.update();
        let update_elapsed = update_start.elapsed();

        if update_elapsed > TICK_DURATION {
            warn!(
                "tick {} took {:.2}ms (exceeded {:.2}ms budget)",
                frame,
                update_elapsed.as_secs_f64() * 1000.0,
                TICK_DURATION.as_secs_f64() * 1000.0
            );
        }

        frame += 1;
    }
}
