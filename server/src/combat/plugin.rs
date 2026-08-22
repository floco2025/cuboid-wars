use bevy::prelude::*;

use super::*;
use crate::schedule::ServerSet;

pub fn combat_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            actors_beam_damage_system.in_set(ServerSet::CombatDamage),
            explosions_system.in_set(ServerSet::CombatExplosions),
        ),
    );
}
