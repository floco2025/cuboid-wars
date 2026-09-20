use bevy::prelude::*;
use common::protocol::server_tick_advance_system;

use super::{
    behavior::{flying_actors_behavior_system, surface_actors_behavior_system},
    navigation::{
        air::AirHomes,
        surface::{SurfaceNavigation, surface_navigation_sync_system},
    },
    *,
};
use crate::{players::players_respawn_system, schedule::ServerSet};

pub fn actors_plugin(app: &mut App) {
    app.init_resource::<AirHomes>().init_resource::<SurfaceNavigation>();
    app.add_systems(
        Update,
        (
            // After the advance, so a spawn due this tick materializes now.
            actors_pending_spawn_system
                .run_if(pending_actor_spawns_active)
                .in_set(ServerSet::Prepare)
                .after(server_tick_advance_system),
            (
                surface_navigation_sync_system,
                surface_actors_behavior_system,
                stationary_actors_behavior_system,
                flying_actors_behavior_system,
            )
                .chain()
                .in_set(ServerSet::Behavior),
            actors_fall_damage_system.in_set(ServerSet::CombatDamage),
            actors_removal_system.in_set(ServerSet::CombatRemoval),
            actors_respawn_system
                .in_set(ServerSet::Lifecycle)
                .after(players_respawn_system),
        ),
    );
}
