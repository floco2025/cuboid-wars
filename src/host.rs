use std::{net::SocketAddr, sync::mpsc::sync_channel, thread};

use anyhow::{Context, Result};
use crossbeam_channel::{Sender, unbounded};

use client::network::ServerLink;
use common::protocol::ClientMessage;
use server::{
    app::{ServerAppOptions, build_server_app, run_server_loop},
    network::{ClientLink, NewLinksChannel, listen},
};

// Runs the server on its own thread and returns the local client's ends of its
// queues. The thread builds the app itself, since Bevy's `App` is not `Send`,
// and reports the build before entering the loop so a bad map or a busy port
// fails here rather than as a hung login. `bind` is `None` for single-player.
pub fn spawn_embedded_server(
    options: ServerAppOptions,
    bind: Option<SocketAddr>,
) -> Result<(Sender<ClientMessage>, ServerLink)> {
    let (to_client, from_server) = unbounded();
    let (to_server, from_client) = unbounded();
    let (register, new_links) = unbounded();
    register
        .send(ClientLink { to_client, from_client })
        .expect("fresh link queue rejected the local link");
    let (built, build_result) = sync_channel(1);
    thread::Builder::new().name("server".into()).spawn(move || {
        let app = bind
            .map(listen)
            .transpose()
            .and_then(|listener| build_server_app(options, NewLinksChannel::new(new_links), listener));
        match app {
            Ok(app) => {
                let _ = built.send(Ok(()));
                run_server_loop(app)
            }
            Err(error) => {
                let _ = built.send(Err(error));
            }
        }
    })?;
    build_result
        .recv()
        .context("server thread stopped before reporting")??;
    Ok((to_server, ServerLink::Local(from_server)))
}
