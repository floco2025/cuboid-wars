use bevy_ecs::prelude::{Res, ResMut};

use super::CollisionWorld;
use crate::{map::MovingFloors, protocol::ServerTick};

// Puts every moving floor at its pose for this tick, in the runtime state
// and in the collision world. Both sides run it right before character
// movement: a jump probe earlier in the tick still sees the tiles where the
// bodies were left standing, and the movement step sees them where they
// are now.
pub fn moving_floors_advance_system(
    tick: Res<ServerTick>,
    mut floors: ResMut<MovingFloors>,
    mut collision_world: ResMut<CollisionWorld>,
) {
    if floors.is_empty() {
        return;
    }
    floors.advance(tick.0);
    collision_world.set_moving_floor_centers(&floors.collider_centers());
}
