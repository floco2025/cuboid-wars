use bevy::prelude::*;

use super::*;
use crate::schedule::ServerSet;

pub fn missiles_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (missiles_guidance_system, missiles_movement_system)
            .chain_ignore_deferred()
            .in_set(ServerSet::CombatDamage),
    );
}
