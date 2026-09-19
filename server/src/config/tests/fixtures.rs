use std::sync::OnceLock;

use serde_json::Value;

use super::{ServerGameplayConfig, strip_registry};

// The shipped gameplay file is the base of every whole-schema config; each
// test pins the values its assertions depend on, and the map comes from the
// small test map alone, merged over the shipped defaults like a real one.
pub(crate) const GAMEPLAY_JSON: &str = include_str!("../../../../config/server/gameplay.json");
pub(crate) const MAP_JSON: &str = include_str!("fixtures/map.json");

// The shipped defaults without the registry keys.
pub(crate) fn gameplay_defaults() -> Value {
    let mut defaults: Value = serde_json::from_str(GAMEPLAY_JSON).expect("test gameplay config is invalid");
    strip_registry(&mut defaults);
    defaults
}

pub(crate) fn server_config() -> ServerGameplayConfig {
    static CONFIG: OnceLock<ServerGameplayConfig> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let map: Value = serde_json::from_str(MAP_JSON).expect("test map settings are invalid");
            let config = ServerGameplayConfig::from_override("hotel", &gameplay_defaults(), &map)
                .expect("test map settings do not merge");
            config.validate("test: ").expect("test config is invalid");
            config
        })
        .clone()
}
