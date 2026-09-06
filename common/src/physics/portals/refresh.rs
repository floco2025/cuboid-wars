use bevy_ecs::prelude::{Res, ResMut};

use super::PortalSet;
use crate::map::Carriers;

// Puts every carried portal at its carrier's pose for this tick. Both sides
// run it right after the carriers advance and before character movement, so
// the movement step, the hop, and the projectile sweep all see the aperture
// where the carrier is.
pub fn carried_portals_refresh_system(carriers: Res<Carriers>, mut portal_set: ResMut<PortalSet>) {
    if !portal_set.has_carried() {
        return;
    }
    portal_set.refresh(&carriers);
}
