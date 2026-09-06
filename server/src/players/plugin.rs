use bevy::prelude::*;

use super::*;
use crate::{items::item_collection_system, schedule::ServerSet};

pub fn players_plugin(app: &mut App) {
    app.init_resource::<EraserContacts>().add_systems(
        Update,
        (
            players_status_timers_system.in_set(ServerSet::Prepare),
            (players_fall_damage_system, players_fall_death_system)
                .chain_ignore_deferred()
                .in_set(ServerSet::CombatDamage),
            players_respawn_system.in_set(ServerSet::Lifecycle),
            erase_equipment_system
                .in_set(ServerSet::Maintenance)
                .after(item_collection_system),
        ),
    );
}
