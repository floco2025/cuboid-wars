use bevy::prelude::*;

use super::*;
use crate::schedule::ServerSet;
use common::physics::powered_bridges_sync_system;

pub fn map_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            weather_system.run_if(weather_needs_tick),
            light_cycle_system.run_if(light_cycle_is_running),
            pressure_plates_system,
            powered_bridges_sync_system.after(pressure_plates_system),
        )
            .in_set(ServerSet::Prepare),
    );
}
