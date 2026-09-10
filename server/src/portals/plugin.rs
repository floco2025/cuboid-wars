use bevy::prelude::*;

use super::equipment::unequipped_portals_cleanup_system;
use crate::{
    characters::characters_movement_system,
    players::{erase_equipment_system, players_status_timers_system},
    schedule::ServerSet,
};
use common::physics::{carried_portals_refresh_system, carriers_advance_system};

pub fn portals_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            unequipped_portals_cleanup_system
                .in_set(ServerSet::Prepare)
                .after(players_status_timers_system),
            unequipped_portals_cleanup_system
                .in_set(ServerSet::Maintenance)
                .after(erase_equipment_system),
            carried_portals_refresh_system
                .in_set(ServerSet::Movement)
                .after(carriers_advance_system)
                .before(characters_movement_system),
        ),
    );
}
