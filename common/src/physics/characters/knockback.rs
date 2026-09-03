use bevy_ecs::prelude::{Query, Res};
use bevy_time::Time;

use crate::protocol::MapSettings;

use super::types::KnockbackVelocity;

pub fn knockback_decay_system(
    time: Res<Time>,
    map_settings: Option<Res<MapSettings>>,
    mut knockbacks: Query<&mut KnockbackVelocity>,
) {
    let Some(map_settings) = map_settings else {
        return;
    };
    let delta = time.delta_secs();
    for mut knockback in &mut knockbacks {
        knockback.decay(delta, map_settings.movement.knockback.deceleration);
    }
}
