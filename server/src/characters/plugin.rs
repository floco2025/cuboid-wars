use bevy::prelude::*;
use common::{physics::carriers_advance_system, protocol::ActorMarker};

use super::contact_explosions::contact_explosions_system;
use super::*;
use crate::schedule::ServerSet;

pub fn characters_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            (
                carriers_advance_system.before(characters_movement_system),
                characters_movement_system,
                contact_explosions_system.after(characters_movement_system),
                knockback_decay_system::<With<ActorMarker>>.after(characters_movement_system),
            )
                .in_set(ServerSet::Movement),
            characters_health_regeneration_system.in_set(ServerSet::Lifecycle),
        ),
    );
}
