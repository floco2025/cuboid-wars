use bevy::prelude::*;

use super::{PendingProjectileHits, hits::projectile_hits_system};
use crate::schedule::ServerSet;

pub fn projectiles_plugin(app: &mut App) {
    app.init_resource::<PendingProjectileHits>()
        .add_systems(Update, projectile_hits_system.in_set(ServerSet::CombatDamage));
}
