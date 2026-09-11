use std::fs;

use anyhow::Result;
use bevy::prelude::App;
use rand::random;
use serde_json::json;

use crossbeam_channel::{Receiver, Sender, unbounded};

use super::{NetworkOverrides, ServerAppOptions, build_server_app_with_loader};
use crate::{
    config::fixtures::server_config,
    map::generation::generate_map_at,
    network::{ClientLink, Listener, NewLinksChannel},
};
use common::protocol::{ClientMessage, ServerMessage};

// A client's ends of the queues its `ClientLink` registers.
pub(crate) fn connect(register: &Sender<ClientLink>) -> (Sender<ClientMessage>, Receiver<ServerMessage>) {
    let (to_client, from_server) = unbounded();
    let (to_server, from_client) = unbounded();
    register
        .send(ClientLink { to_client, from_client })
        .expect("registration queue closed");
    (to_server, from_server)
}

pub(crate) fn server_app(overrides: NetworkOverrides, new_links: NewLinksChannel) -> Result<App> {
    server_app_with_listener(overrides, new_links, None)
}

pub(crate) fn server_app_with_listener(
    overrides: NetworkOverrides,
    new_links: NewLinksChannel,
    listener: Option<Listener>,
) -> Result<App> {
    let mut config = server_config();
    for map in config.maps.values_mut() {
        map.random_items = None;
    }
    let options = ServerAppOptions {
        map: None,
        network: overrides,
        logging: false,
    };
    build_server_app_with_loader(config, options, new_links, listener, |name, hz, settings| {
        let directory = std::env::temp_dir().join(format!("cuboid_app_{}", random::<u64>()));
        fs::create_dir(&directory)?;
        let path = directory.join("layout.json");
        let source = json!({"map": {
            "grid_cols": 2, "grid_rows": 2,
            "levels": [{"floors": [
                {"col": 0, "row": 0, "all": "basement-floor"},
                {"col": 1, "row": 0, "all": "basement-floor"},
                {"col": 0, "row": 1, "all": "basement-floor"},
                {"col": 1, "row": 1, "all": "basement-floor"}
            ]}],
            "player_spawn_zones": [{"level": 0, "cols": [0, 2], "rows": [0, 2]}]
        }});
        fs::write(&path, source.to_string())?;
        let generated = generate_map_at(&path, name, hz, settings);
        fs::remove_dir_all(directory)?;
        generated
    })
}
