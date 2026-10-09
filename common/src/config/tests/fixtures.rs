use anyhow::Result;

use super::{CharacterPhysicsConfig, GameplayConfig};

pub(crate) fn load_test_gameplay() -> Result<GameplayConfig> {
    let config: GameplayConfig = serde_json::from_str(include_str!("fixtures/gameplay.json"))?;
    config.validate()?;
    Ok(config)
}

pub(crate) fn player_physics() -> CharacterPhysicsConfig {
    load_test_gameplay()
        .expect("test gameplay config rejected")
        .player
        .physics()
}
