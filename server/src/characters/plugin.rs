use bevy::prelude::*;
use common::{physics::carriers_advance_system, protocol::ActorMarker};

use super::contact_explosions::contact_explosions_system;
use super::*;
use crate::{
    actors::{anchored_actors_placement_system, surface_actors_movement_system},
    schedule::ServerSet,
};

pub fn characters_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            (
                carriers_advance_system,
                anchored_actors_placement_system,
                surface_actors_movement_system,
                flying_actors_movement_system,
                contact_explosions_system,
                knockback_decay_system::<With<ActorMarker>>,
            )
                .chain()
                .in_set(ServerSet::Movement),
            characters_health_regeneration_system.in_set(ServerSet::Lifecycle),
        ),
    );
}
