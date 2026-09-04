use bevy::prelude::*;

use super::*;
use crate::schedule::ServerSet;

pub fn actors_plugin(app: &mut App) {
    app.add_systems(Startup, actors_initial_spawn_system).add_systems(
        Update,
        (
            actors_pending_spawn_system
                .run_if(pending_actor_spawns_active)
                .in_set(ServerSet::Prepare),
            actors_behavior_system.in_set(ServerSet::Behavior),
            actors_removal_system.in_set(ServerSet::CombatRemoval),
            actors_respawn_system
                .run_if(actor_respawns_active)
                .in_set(ServerSet::Lifecycle),
        ),
    );
}
