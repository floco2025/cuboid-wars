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
        args.server_hz,
        args.update_hz,
        args.snapshot_hz,
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
mod tests {
    use super::*;

    #[test]
    fn rate_overrides_accept_positive_integers_and_validate_relationships_after_loading_config() {
        let defaults = Args::try_parse_from(["server"]).expect("default arguments invalid");
        assert!(defaults.server_hz.is_none());
        assert!(defaults.update_hz.is_none());
        assert!(defaults.snapshot_hz.is_none());
        for option in ["--server-hz", "--update-hz", "--snapshot-hz"] {
            for hz in ["1", "7", "30", "60"] {
                let args = Args::try_parse_from(["server", option, hz]).expect("rate rejected");
                assert_eq!(
                    args.update_hz.or(args.snapshot_hz).or(args.server_hz),
                    Some(hz.parse().expect("test rate invalid"))
                );
            }
            for hz in ["0", "-1", "10.5"] {
                assert!(Args::try_parse_from(["server", option, hz]).is_err());
            }
        }
        assert!(Args::try_parse_from(["server", "--update-hz", "1", "--snapshot-hz", "30"]).is_ok());
    }
}
