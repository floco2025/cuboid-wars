use std::{net::SocketAddr, process, time::Duration};

use anyhow::Result;
use bevy::app::AppExit;
use clap::{Args, Parser, Subcommand};
use crossbeam_channel::{Sender, unbounded};

use client::{
    app::{ClientAppOptions, build_client_app},
    network::{ClientToServerChannel, Impairment, ServerLink, connect, login},
};
use common::protocol::ClientMessage;
use server::{
    app::{NetworkOverrides, ServerAppOptions, build_server_app, run_server_loop},
    network::{NewLinksChannel, listen},
};

use crate::host::spawn_embedded_server;

mod host;

const DEFAULT_ADDRESS: &str = "127.0.0.1:8080";

// Without a subcommand the game is single-player: a window and an embedded
// server that listens to nobody.
#[derive(Parser)]
#[command(author, version, about = "Cuboid Wars", long_about = None, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    mode: Option<Mode>,
    #[command(flatten)]
    window: WindowArgs,
    #[command(flatten)]
    world: WorldArgs,
}

#[derive(Subcommand)]
enum Mode {
    /// Play and let others join over the network.
    Host {
        #[arg(short, long, default_value = DEFAULT_ADDRESS)]
        bind: SocketAddr,
        #[command(flatten)]
        window: WindowArgs,
        #[command(flatten)]
        world: WorldArgs,
    },
    /// Join a server.
    Join {
        #[arg(default_value = DEFAULT_ADDRESS)]
        server: SocketAddr,
        #[command(flatten)]
        impairment: ImpairmentArgs,
        #[command(flatten)]
        window: WindowArgs,
    },
    /// Run a headless server.
    Serve {
        #[arg(short, long, default_value = DEFAULT_ADDRESS)]
        bind: SocketAddr,
        #[command(flatten)]
        world: WorldArgs,
    },
}

#[derive(Args, Debug)]
struct WindowArgs {
    #[arg(short, long)]
    name: Option<String>,

    // Position uses macOS points or Windows/X11 pixels; Wayland chooses placement.
    #[arg(long)]
    window_x: Option<i32>,

    #[arg(long)]
    window_y: Option<i32>,

    // Windowed size in logical pixels; each axis defaults to the saved size.
    #[arg(long)]
    window_width: Option<u32>,

    #[arg(long)]
    window_height: Option<u32>,

    #[arg(long)]
    volume: Option<f32>,
}

impl WindowArgs {
    fn player_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            let full_name = whoami::realname().unwrap_or_default();
            full_name.split_whitespace().next().unwrap_or_default().to_string()
        })
    }

    fn client_options(&self, logging: bool) -> ClientAppOptions {
        ClientAppOptions {
            window_x: self.window_x,
            window_y: self.window_y,
            window_width: self.window_width,
            window_height: self.window_height,
            volume: self.volume,
            logging,
        }
    }
}

#[derive(Args, Debug)]
struct WorldArgs {
    #[arg(long)]
    map: Option<String>,

    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    server_hz: Option<u32>,

    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    update_hz: Option<u32>,

    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    snapshot_hz: Option<u32>,
}

impl WorldArgs {
    // The server app is always the first one built, so it owns the log plugin.
    fn server_options(&self) -> ServerAppOptions {
        ServerAppOptions {
            map: self.map.clone(),
            network: NetworkOverrides {
                server_hz: self.server_hz,
                update_hz: self.update_hz,
                snapshot_hz: self.snapshot_hz,
            },
            logging: true,
        }
    }
}

#[derive(Args, Debug)]
struct ImpairmentArgs {
    /// Simulated one-way delay in milliseconds, applied in both directions.
    #[arg(long, default_value = "0")]
    lag_ms: u64,

    /// Unreliable delay variation as a fraction of lag; 0 disables it, 0.5 varies by ±50%.
    #[arg(long, default_value = "0.05", value_parser = parse_fraction)]
    jitter: f32,

    /// Fraction of unreliable messages to discard, sent and received.
    #[arg(long, default_value = "0", value_parser = parse_fraction)]
    drop: f32,
}

impl ImpairmentArgs {
    fn impairment(&self) -> Impairment {
        Impairment {
            lag: Duration::from_millis(self.lag_ms),
            jitter: self.jitter,
            drop_probability: self.drop,
        }
    }
}

fn parse_fraction(value: &str) -> Result<f32, String> {
    let fraction = value.parse::<f32>().map_err(|error| error.to_string())?;
    if !(0.0..=1.0).contains(&fraction) {
        return Err("must be a finite fraction in 0..=1".to_owned());
    }
    Ok(fraction)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.mode {
        None => {
            let (to_server, link) = spawn_embedded_server(cli.world.server_options(), None)?;
            play(&cli.window, to_server, link, false)
        }
        Some(Mode::Host { bind, window, world }) => {
            let (to_server, link) = spawn_embedded_server(world.server_options(), Some(bind))?;
            play(&window, to_server, link, false)
        }
        Some(Mode::Join {
            server,
            impairment,
            window,
        }) => {
            let (to_server, link) = connect(server, impairment.impairment())?;
            play(&window, to_server, link, true)
        }
        Some(Mode::Serve { bind, world }) => {
            let (_register, new_links) = unbounded();
            let listener = listen(bind)?;
            let app = build_server_app(world.server_options(), NewLinksChannel::new(new_links), Some(listener))?;
            run_server_loop(app)
        }
    }
}

// Logs in over the link, builds the client app around it, and runs it on
// this thread; `logging` is false when an embedded server owns the log plugin.
// Bevy's default plugins turn Ctrl+C into an `AppExit`, and the network
// plugin tells the server on the frame the app exits.
fn play(window: &WindowArgs, to_server: Sender<ClientMessage>, mut link: ServerLink, logging: bool) -> Result<()> {
    let bootstrap = login(&mut link, &to_server, window.player_name())?;
    let mut app = build_client_app(
        window.client_options(logging),
        ClientToServerChannel::new(to_server),
        link,
        bootstrap,
    )?;
    let exit = app.run();
    // Winit does not always finish tearing down after AppExit on macOS.
    process::exit(match exit {
        AppExit::Success => 0,
        AppExit::Error(code) => i32::from(code.get()),
    });
}

#[cfg(test)]
#[path = "tests/main.rs"]
mod tests;
