use std::{net::SocketAddr, sync::mpsc::sync_channel, thread};

use anyhow::{Context, Result};
use tokio::{
    runtime::Handle,
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
};

use common::protocol::{ClientMessage, ServerMessage};
use server::{
    app::{ServerAppOptions, build_server_app, run_server_loop},
    network::{ClientLink, NewLinksChannel, listen},
};

// Runs the server on its own thread and returns the local client's ends of its
// queues. The thread builds the app itself, since Bevy's `App` is not `Send`,
// and reports the build before entering the loop so a bad map fails here
// rather than as a hung login. `bind` is `None` for single-player.
pub fn spawn_embedded_server(
    handle: &Handle,
    options: ServerAppOptions,
    bind: Option<SocketAddr>,
) -> Result<(UnboundedSender<ClientMessage>, UnboundedReceiver<ServerMessage>)> {
    let (to_client, from_server) = unbounded_channel();
    let (to_server, from_client) = unbounded_channel();
    let (register, new_links) = unbounded_channel();
    register
        .send(ClientLink { to_client, from_client })
        .expect("fresh link queue rejected the local link");
    let (built, build_result) = sync_channel(1);
    let handle = handle.clone();
    thread::Builder::new().name("server".into()).spawn(move || {
        let app = build_server_app(options, NewLinksChannel::new(new_links)).and_then(|app| {
            if let Some(bind) = bind {
                listen(&handle, bind, register)?;
            }
            Ok(app)
        });
        match app {
            Ok(app) => {
                let _ = built.send(Ok(()));
                run_server_loop(app, &handle)
            }
            Err(error) => {
                let _ = built.send(Err(error));
            }
        }
    })?;
    build_result
        .recv()
        .context("server thread stopped before reporting")??;
    Ok((to_server, from_server))
}
