use bevy::prelude::*;

use super::*;
use crate::schedule::ServerSet;

pub fn players_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            players_status_timers_system.in_set(ServerSet::Prepare),
            (players_fall_damage_system, players_fall_death_system)
                .chain_ignore_deferred()
                .in_set(ServerSet::CombatDamage),
            players_respawn_system.in_set(ServerSet::Lifecycle),
        ),
    );
}
