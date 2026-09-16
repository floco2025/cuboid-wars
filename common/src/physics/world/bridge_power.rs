use bevy_ecs::{
    change_detection::DetectChanges,
    prelude::{Res, ResMut},
};

use super::CollisionWorld;
use crate::protocol::SwitchState;

// Applies powered bridge instances to the collision world. The server runs
// it right after the plate system, the client right after a snapshot lands,
// so every surface query that follows sees the current bridges.
pub fn powered_bridges_sync_system(switch_state: Res<SwitchState>, mut collision_world: ResMut<CollisionWorld>) {
    if switch_state.is_changed() {
        collision_world.set_powered_bridges(&switch_state.powered_bridges);
    }
}
