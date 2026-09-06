use bevy::prelude::*;

use super::players_portal_traversal_system;
use crate::{
    characters::{characters_movement_system, knockback_decay_system},
    schedule::ServerSet,
};
use common::physics::{carried_portals_refresh_system, carriers_advance_system};

pub fn portals_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            carried_portals_refresh_system
                .in_set(ServerSet::Movement)
                .after(carriers_advance_system)
                .before(characters_movement_system),
            players_portal_traversal_system
                .in_set(ServerSet::Movement)
                // The hop reads the same knockback value movement integrated, so
                // it must land between the step and the decay.
                .after(characters_movement_system)
                .before(knockback_decay_system),
        ),
    );
}
