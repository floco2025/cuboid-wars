use bevy::prelude::*;

use super::equipment::{erased_portals_system, unequipped_portals_cleanup_system};
use crate::{
    items::item_collection_system,
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
            erased_portals_system
                .in_set(ServerSet::Maintenance)
                .after(item_collection_system)
                .before(erase_equipment_system),
            unequipped_portals_cleanup_system
                .in_set(ServerSet::Maintenance)
                .after(erase_equipment_system),
        ),
    );
}
