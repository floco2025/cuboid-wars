use bevy::prelude::*;

use super::*;
use super::{
    pressure_plates::{pressure_plates_system, pressure_switch_death_reset_system},
    pressure_switches::{PressureSwitches, plate_state_sync_system},
};
use crate::{
    players::{players_group_respawn_system, players_respawn_system},
    schedule::ServerSet,
};
use common::physics::powered_bridges_sync_system;

pub fn map_plugin(app: &mut App) {
    app.init_resource::<PressureSwitches>()
        .add_systems(
            Update,
            (
                weather_system.run_if(weather_needs_tick),
                light_cycle_system.run_if(light_cycle_is_running),
                (
                    pressure_plates_system,
                    plate_state_sync_system,
                    powered_bridges_sync_system,
                )
                    .chain(),
            )
                .in_set(ServerSet::Prepare),
        )
        .add_systems(
            Update,
            (
                pressure_switch_death_reset_system,
                plate_state_sync_system,
                powered_bridges_sync_system,
            )
                .chain()
                .in_set(ServerSet::Lifecycle)
                .after(players_group_respawn_system)
                .before(players_respawn_system),
        );
}
