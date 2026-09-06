use bevy_ecs::prelude::{Res, ResMut};

use super::PortalSet;
use crate::map::MovingFloors;

// Puts every tile-anchored portal at its tile's pose for this tick. Both
// sides run it right after the tiles advance and before character movement,
// so the movement step, the hop, and the projectile sweep all see the
// aperture where the tile is.
pub fn anchored_portals_refresh_system(floors: Res<MovingFloors>, mut portal_set: ResMut<PortalSet>) {
    if !portal_set.has_anchored() {
        return;
    }
    portal_set.refresh(&floors);
}
