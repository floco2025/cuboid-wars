use bevy::prelude::*;
use common::physics::moving_floors_advance_system;

use super::*;
use crate::schedule::ServerSet;

pub fn characters_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            (
                moving_floors_advance_system.before(characters_movement_system),
                characters_movement_system,
                knockback_decay_system.after(characters_movement_system),
            )
                .in_set(ServerSet::Movement),
            characters_health_regeneration_system.in_set(ServerSet::Lifecycle),
        ),
    );
}
