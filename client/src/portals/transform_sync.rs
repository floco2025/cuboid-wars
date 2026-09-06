use bevy::prelude::*;

use super::{PortalMap, spawn::PortalSurface};
use crate::constants::PORTAL_SURFACE_OFFSET;
use common::{map::MovingFloors, physics::PortalFrame};

// Every render frame, place each disc of an anchored portal where its tile
// is between the last two ticks, the same interpolation the tile mesh uses,
// so the disc stays on it.
pub fn portal_surfaces_transform_sync_system(
    fixed_time: Res<Time<Fixed>>,
    floors: Res<MovingFloors>,
    portals: Res<PortalMap>,
    mut surfaces: Query<(&PortalSurface, &mut Transform)>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (surface, mut transform) in &mut surfaces {
        let Some(info) = portals.get(&(surface.pair, surface.end)) else {
            continue;
        };
        if info.portal.anchor.is_none() {
            continue;
        }
        let frame = PortalFrame::from_portal_between(&info.portal, &floors, alpha);
        transform.translation = frame.center + frame.normal * PORTAL_SURFACE_OFFSET;
    }
}
