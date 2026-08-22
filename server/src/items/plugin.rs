use bevy::prelude::*;

use super::*;
use crate::schedule::ServerSet;

pub fn items_plugin(app: &mut App) {
    app.add_systems(Startup, placed_item_spawn_system).add_systems(
        Update,
        (
            placed_item_respawn_system,
            random_item_despawn_system,
            item_collection_system,
            random_item_spawn_system,
        )
            .chain_ignore_deferred()
            .in_set(ServerSet::Maintenance),
    );
}
