use bevy::prelude::*;
use common::protocol::PowerUpKind;

use super::{PortalAssignments, PortalMap};
use crate::players::PlayerMap;

pub fn unequipped_portals_cleanup_system(
    players: Res<PlayerMap>,
    assignments: Res<PortalAssignments>,
    mut portals: ResMut<PortalMap>,
) {
    for (id, info) in players.iter() {
        if info.is_dead() || !info.has(PowerUpKind::PortalGun) {
            portals.remove_access(assignments.get(id));
        }
    }
}

#[cfg(test)]
#[path = "tests/equipment.rs"]
mod tests;
