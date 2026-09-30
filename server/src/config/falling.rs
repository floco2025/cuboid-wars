use bevy::prelude::Resource;

pub use common::config::FallDamageConfig;

// The selected map's landing thresholds: `player_fall` for players and
// `actor_fall` for ground actors.
#[derive(Resource, Debug, Clone, Copy)]
pub struct FallDamageConfigs {
    pub player: FallDamageConfig,
    pub actor: FallDamageConfig,
}
