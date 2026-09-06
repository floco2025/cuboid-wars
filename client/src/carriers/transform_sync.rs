use bevy::prelude::*;

use super::CarrierMarker;
use common::map::Carriers;

// Every render frame, place each carrier's root between its last two tick
// poses by the fixed-step overstep fraction, the same interpolation the
// characters use, so a rider and the floor under it stay attached between
// ticks. Everything on the carrier is a child and follows.
pub fn carriers_transform_sync_system(
    fixed_time: Res<Time<Fixed>>,
    carriers: Res<Carriers>,
    mut roots: Query<(&CarrierMarker, &mut Transform)>,
) {
    if carriers.is_static() {
        return;
    }
    let alpha = fixed_time.overstep_fraction();
    for (root, mut transform) in &mut roots {
        if root.id.is_world() {
            continue;
        }
        transform.translation = carriers.pose_between(root.id, alpha).translation;
    }
}
