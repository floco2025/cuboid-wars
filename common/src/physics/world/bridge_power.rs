use bevy_ecs::{
    change_detection::DetectChanges,
    prelude::{Res, ResMut},
};

use super::CollisionWorld;
use crate::protocol::PlateState;

// Applies the powered bridge kinds to the collision world. The server runs
// it right after the plate system, the client right after a snapshot lands,
// so every surface query that follows sees the current bridges.
pub fn powered_bridges_sync_system(plates: Res<PlateState>, mut collision_world: ResMut<CollisionWorld>) {
    if plates.is_changed() {
        collision_world.set_powered_bridges(&plates.powered_bridge_kinds);
    }
}
