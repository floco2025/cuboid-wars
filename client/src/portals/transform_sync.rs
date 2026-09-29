use bevy::prelude::*;

use super::{
    PortalMap,
    spawn::{PORTAL_SURFACE_OFFSET, PortalSurface},
};
use common::{config::GameplayConfig, map::Carriers, physics::PortalFrame};

// Every render frame, place each disc of an anchored portal where its tile
// is between the last two ticks, the same interpolation the tile mesh uses,
// so the disc stays on it.
pub(crate) fn portal_surfaces_transform_sync_system(
    config: Res<GameplayConfig>,
    fixed_time: Res<Time<Fixed>>,
    carriers: Res<Carriers>,
    portals: Res<PortalMap>,
    mut surfaces: Query<(&PortalSurface, &mut Transform)>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (surface, mut transform) in &mut surfaces {
        let Some(info) = portals.get(&(surface.pair, surface.end)) else {
            continue;
        };
        if info.portal.carrier.is_world() {
            continue;
        }
        let frame = PortalFrame::from_portal_between(&info.portal, &carriers, alpha, config.portals.size);
        transform.translation = frame.center + frame.normal * PORTAL_SURFACE_OFFSET;
    }
}
