use bevy::prelude::*;

use super::expiry::missiles_expiry_system;
use crate::schedule::ServerSet;

pub fn missiles_plugin(app: &mut App) {
    app.add_systems(Update, missiles_expiry_system.in_set(ServerSet::Maintenance));
}
