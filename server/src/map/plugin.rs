use bevy::prelude::*;

use super::{
    light_cycle_is_running, light_cycle_system,
    pressure_plates::{
        PressureSwitches, plate_state_sync_system, pressure_plates_system, pressure_switch_reset_system,
    },
    weather_needs_tick, weather_system,
};
use crate::{
    players::{players_group_respawn_system, players_respawn_system},
    schedule::ServerSet,
};
use common::{physics::powered_bridges_sync_system, protocol::server_tick_advance_system};

pub fn map_plugin(app: &mut App) {
    app.init_resource::<PressureSwitches>()
        .add_systems(
            Update,
            (
                weather_system.run_if(weather_needs_tick),
                light_cycle_system.run_if(light_cycle_is_running),
                // Switch flips stamp the tick the carriers advance to later this
                // tick, so the tick must already have advanced.
                (
                    pressure_plates_system,
                    plate_state_sync_system,
                    powered_bridges_sync_system,
                )
                    .chain()
                    .after(server_tick_advance_system),
            )
                .in_set(ServerSet::Prepare),
        )
        .add_systems(
            Update,
            (
                pressure_switch_reset_system,
                plate_state_sync_system,
                powered_bridges_sync_system,
            )
                .chain()
                .in_set(ServerSet::Lifecycle)
                .after(players_group_respawn_system)
                .before(players_respawn_system),
        );
}
