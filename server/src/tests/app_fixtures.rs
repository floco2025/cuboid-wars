use std::fs;

use anyhow::Result;
use bevy::prelude::App;
use rand::random;
use serde_json::json;

use super::{NetworkOverrides, build_server_app_with_loader};
use crate::{config::fixtures::server_config, map::generation::generate_map_at, network::FromClientsChannel};

pub(super) fn server_app(overrides: NetworkOverrides, from_clients: FromClientsChannel) -> Result<App> {
    let mut config = server_config();
    for map in config.maps.values_mut() {
        map.random_items = None;
    }
    build_server_app_with_loader(config, None, overrides, from_clients, |name, hz, settings| {
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
