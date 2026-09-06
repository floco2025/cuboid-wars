use bevy_ecs::prelude::{Res, ResMut};

use super::CollisionWorld;
use crate::{map::Carriers, protocol::ServerTick};

// Puts every carrier at its pose for this tick, in the runtime state and in
// the collision world. Both sides run it right before character movement: a
// jump probe earlier in the tick still sees the carriers where the bodies
// were left standing, and the movement step sees them where they are now.
pub fn carriers_advance_system(
    tick: Res<ServerTick>,
    mut carriers: ResMut<Carriers>,
    mut collision_world: ResMut<CollisionWorld>,
) {
    if carriers.is_static() {
        return;
    }
    carriers.advance(tick.0);
    collision_world.set_carrier_poses(&carriers);
}
