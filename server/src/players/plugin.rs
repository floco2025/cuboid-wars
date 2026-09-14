use bevy::prelude::*;

use super::*;
use crate::{characters::characters_movement_system, items::item_collection_system, schedule::ServerSet};

pub fn players_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            players_status_timers_system.in_set(ServerSet::Prepare),
            apply_player_movement_system
                .in_set(ServerSet::Movement)
                .before(characters_movement_system),
            (players_fall_damage_system, players_fatal_outcomes_system)
                .chain_ignore_deferred()
                .in_set(ServerSet::CombatDamage),
            (
                players_group_respawn_system,
                players_checkpoints_system.run_if(checkpoints_exist),
                players_respawn_system,
            )
                .chain()
                .in_set(ServerSet::Lifecycle),
            erase_equipment_system
                .in_set(ServerSet::Maintenance)
                .after(item_collection_system),
        ),
    );
}
