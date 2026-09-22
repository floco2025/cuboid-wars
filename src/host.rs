use std::{net::SocketAddr, sync::mpsc::sync_channel, thread};

use anyhow::{Context, Result};
use bevy::prelude::App;
use crossbeam_channel::{Sender, unbounded};

use client::network::ServerLink;
use common::protocol::ClientMessage;
use server::{
    app::{ServerAppOptions, build_server_app, run_server_loop},
    network::{LocalLink, listen},
};

pub fn spawn_embedded_server(
    options: ServerAppOptions,
    bind: Option<SocketAddr>,
) -> Result<(Sender<ClientMessage>, ServerLink)> {
    spawn_local_server(move |local| {
        let listener = bind.map(listen).transpose()?;
        build_server_app(options, listener, Some(local))
    })
}

// The thread builds the app itself because Bevy's App is not Send. Returning
// the build result before login exposes map and listener errors without a hang.
pub fn spawn_local_server(
    build: impl FnOnce(LocalLink) -> Result<App> + Send + 'static,
) -> Result<(Sender<ClientMessage>, ServerLink)> {
    let (to_client, from_server) = unbounded();
    let (to_server, from_client) = unbounded();
    let local = LocalLink { to_client, from_client };
    let (built, build_result) = sync_channel(1);
    thread::Builder::new()
        .name("server".into())
        .spawn(move || match build(local) {
            Ok(app) => {
                let _ = built.send(Ok(()));
                run_server_loop(app)
            }
            Err(error) => {
                let _ = built.send(Err(error));
            }
        })?;
    build_result
        .recv()
        .context("server thread stopped before reporting")??;
    Ok((to_server, ServerLink::Local(from_server)))
}
