use bevy::prelude::*;

use super::MovingFloorMarker;
use common::map::MovingFloors;

// Every render frame, place each tile between its last two tick poses by
// the fixed-step overstep fraction, the same interpolation the characters
// use, so a rider and the tile under it stay attached between ticks.
pub fn moving_floors_transform_sync_system(
    fixed_time: Res<Time<Fixed>>,
    floors: Res<MovingFloors>,
    mut tiles: Query<(&MovingFloorMarker, &mut Transform)>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (tile, mut transform) in &mut tiles {
        if let Some(center) = floors.interpolated_surface_center(tile.index, alpha) {
            transform.translation = center;
        }
    }
}
