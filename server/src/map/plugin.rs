use bevy::prelude::*;

use super::*;
use crate::schedule::ServerSet;

pub fn map_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (weather_system, compute_open_barrier_kinds_system).in_set(ServerSet::Prepare),
    );
}
