use bevy::prelude::*;
use common::protocol::{SEquipmentErased, ServerMessage};

use super::PlayerMap;
use crate::network::{ServerToClient, broadcast_to_all};

pub fn erase_equipment_system(mut players: ResMut<PlayerMap>) {
    let mut statuses = Vec::new();
    for (id, info) in players.iter_mut() {
        if info.is_dead() {
            continue;
        }
        let erase = std::mem::take(&mut info.life.outcomes.erase_equipment);
        let erased = erase && info.erase_equipment();
        if erased && info.connection.logged_in {
            let _ = info
                .connection
                .channel
                .send(ServerToClient::Send(ServerMessage::EquipmentErased(SEquipmentErased)));
        }
        if erased {
            statuses.push(info.status(*id));
        }
    }
    for status in statuses {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(status));
    }
}

#[cfg(test)]
#[path = "tests/equipment.rs"]
mod tests;
