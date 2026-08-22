use bevy::prelude::*;

use super::*;
use crate::schedule::ServerSet;

pub fn projectiles_plugin(app: &mut App) {
    app.add_systems(Update, projectiles_movement_system.in_set(ServerSet::CombatDamage));
}
