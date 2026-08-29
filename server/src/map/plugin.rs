use bevy::prelude::*;

use super::*;
use crate::schedule::ServerSet;

pub fn map_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (weather_system, light_cycle_system, pressure_plates_system).in_set(ServerSet::Prepare),
    );
}
