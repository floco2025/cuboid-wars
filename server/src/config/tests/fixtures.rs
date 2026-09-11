use crate::config::MapServerConfig;

use super::{GameplayFile, ServerGameplayConfig};

// The shipped gameplay file is the base of every whole-schema config; each
// test pins the values its assertions depend on, and maps come from the
// small test map alone.
pub(crate) const GAMEPLAY_JSON: &str = include_str!("../../../../config/server/gameplay.json");
pub(crate) const MAP_JSON: &str = include_str!("fixtures/map.json");

pub(crate) fn server_config() -> ServerGameplayConfig {
    let source: GameplayFile = serde_json::from_str(GAMEPLAY_JSON).expect("test gameplay config is invalid");
    let map: MapServerConfig = serde_json::from_str(MAP_JSON).expect("test map settings are invalid");
    ServerGameplayConfig {
        network: source.network,
        default_map: source.default_map,
        maps: source.maps.into_iter().map(|name| (name, map.clone())).collect(),
        player: source.player,
        actors: source.actors,
        weapons: source.weapons,
        combat: source.combat,
        scoring: source.scoring,
        cycles: source.cycles,
        feed: source.feed,
    }
}
