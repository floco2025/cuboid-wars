use bevy::prelude::*;

use super::{
    pressure_plates::{PressurePlateInputs, pressure_plates_system, switch_reset_system},
    switches::{Switches, switch_state_sync_system},
    weather_system,
};
use crate::{
    players::{players_group_respawn_system, players_respawn_system},
    schedule::ServerSet,
};
use common::{physics::powered_bridges_sync_system, protocol::server_tick_advance_system};

pub fn map_plugin(app: &mut App) {
    app.init_resource::<Switches>()
        .init_resource::<PressurePlateInputs>()
        .add_systems(
            Update,
            (
                weather_system,
                // Switch flips stamp the tick the carriers advance to later this
                // tick, so the tick must already have advanced.
                (
                    pressure_plates_system,
                    switch_state_sync_system,
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
                switch_reset_system,
                switch_state_sync_system,
                powered_bridges_sync_system,
            )
                .chain()
                .in_set(ServerSet::Lifecycle)
                .after(players_group_respawn_system)
                .before(players_respawn_system),
        );
}
