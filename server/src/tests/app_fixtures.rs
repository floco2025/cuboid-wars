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
    network::{Listener, LocalLink, register_local},
};
use common::protocol::{ClientMessage, ServerMessage};

// Registers a local client and returns its ends of the queues.
pub(crate) fn connect(app: &mut App) -> (Sender<ClientMessage>, Receiver<ServerMessage>) {
    let (to_client, from_server) = unbounded();
    let (to_server, from_client) = unbounded();
    register_local(app.world_mut(), LocalLink { to_client, from_client });
    (to_server, from_server)
}

pub(crate) fn server_app(overrides: NetworkOverrides) -> Result<App> {
    server_app_with_listener(overrides, None)
}

pub(crate) fn server_app_with_listener(overrides: NetworkOverrides, listener: Option<Listener>) -> Result<App> {
    let options = ServerAppOptions {
        map: None,
        god: false,
        peace: false,
        network: overrides,
        logging: false,
    };
    server_app_with_options(options, listener)
}

pub(crate) fn server_app_with_options(options: ServerAppOptions, listener: Option<Listener>) -> Result<App> {
    let mut config = server_config();
    for map in config.maps.values_mut() {
        map.random_items = None;
    }
    build_server_app_with_loader(config, options, listener, None, |name, hz, settings| {
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
