use bevy::prelude::*;
use common::protocol::{PowerUpKind, SEquipmentErased, ServerMessage};

use super::{PortalAssignments, PortalMap};
use crate::players::PlayerMap;

// An eraser closes the portals of the player it erases, before the equipment
// goes; a portal gun the map hands out for good stays. When the portals are
// all it takes, the erasure cue goes from here, otherwise with the equipment.
pub fn erased_portals_system(
    players: Res<PlayerMap>,
    assignments: Res<PortalAssignments>,
    mut portals: ResMut<PortalMap>,
) {
    for (id, info) in players.iter() {
        if info.is_dead() || !info.life.outcomes.erase_equipment {
            continue;
        }
        if portals.remove_access(assignments.get(id)) && !info.has_erasable_equipment() && info.connection.logged_in {
            let _ = info
                .connection
                .channel
                .send(ServerMessage::EquipmentErased(SEquipmentErased));
        }
    }
}

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
