use bevy::prelude::*;
use common::{physics::CollisionWorld, protocol::SwitchId};

use super::{PlateSwitchMarker, PressurePlateMarker};

#[derive(Resource, Default, PartialEq, Eq)]
pub struct LockedSwitches(pub Vec<SwitchId>);

pub(super) fn plate_visibility(switch: SwitchId, locked: &[SwitchId]) -> Visibility {
    if locked.contains(&switch) {
        Visibility::Hidden
    } else {
        Visibility::Visible
    }
}

pub fn pressure_plates_visibility_system(
    locked: Res<LockedSwitches>,
    mut collision_world: ResMut<CollisionWorld>,
    mut plates: Query<(&PlateSwitchMarker, &mut Visibility), With<PressurePlateMarker>>,
) {
    if !locked.is_changed() {
        return;
    }
    // Visibility and collision change together before the next fixed step.
    collision_world.set_locked_pressure_plates(&locked.0);
    for (switch, mut visibility) in &mut plates {
        // Equal writes would retrigger visibility propagation for unrelated switches.
        visibility.set_if_neq(plate_visibility(switch.0, &locked.0));
    }
}
