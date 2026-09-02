use bevy_ecs::prelude::{Query, Res};
use bevy_time::Time;

use crate::config::GameplayConfig;

use super::types::KnockbackVelocity;

pub fn knockback_decay_system(
    time: Res<Time>,
    gameplay_config: Res<GameplayConfig>,
    mut knockbacks: Query<&mut KnockbackVelocity>,
) {
    let delta = time.delta_secs();
    for mut knockback in &mut knockbacks {
        knockback.decay(delta, gameplay_config.movement.knockback.deceleration);
    }
}
