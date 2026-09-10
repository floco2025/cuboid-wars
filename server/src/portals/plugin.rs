use bevy::prelude::*;

use super::equipment::unequipped_portals_cleanup_system;
use crate::{
    players::{erase_equipment_system, players_status_timers_system},
    schedule::ServerSet,
};

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
        ),
    );
}
