use bevy::prelude::*;
use std::collections::HashSet;

use super::super::components::ServerReconciliation;
use crate::{
    missiles::{MissileAssets, MissileMap, MissileVelocity, spawn_missile},
    network::RoundTripTime,
};
use common::protocol::{Missile, MissileId, MissileMarker, MissileMovementState, Position};

// Snapshot diff for missiles, the same idiom as `sync_actors`: spawn ids the
// snapshot has and we don't, silently despawn ids it dropped (the
// `SMissileDeath` cue owns the detonation VFX), then apply the carried
// movement as a reconciliation target.
pub fn sync_missiles(
    commands: &mut Commands,
    missile_assets: &MissileAssets,
    missiles: &mut MissileMap,
    rtt: &ResMut<RoundTripTime>,
    missile_data: &Query<&Position, With<MissileMarker>>,
    server_missiles: &[(MissileId, Missile)],
) {
    let update_ids: HashSet<MissileId> = server_missiles.iter().map(|(id, _)| *id).collect();

    for (id, missile) in server_missiles {
        if missiles.contains_key(id) {
            continue;
        }
        let entity = spawn_missile(commands, missile_assets, *id, &missile.movement);
        missiles.insert(*id, entity);
    }

    missiles.retain(|id, entity| {
        if update_ids.contains(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    for (id, missile) in server_missiles {
        apply_missile_movement_state(commands, missiles, rtt, missile_data, *id, missile.movement);
    }
}

pub(super) fn apply_missile_movement_state(
    commands: &mut Commands,
    missiles: &MissileMap,
    rtt: &ResMut<RoundTripTime>,
    missile_data: &Query<&Position, With<MissileMarker>>,
    id: MissileId,
    movement: MissileMovementState,
) {
    let Some(entity) = missiles.get(&id) else {
        return;
    };
    let velocity = movement.velocity();
    commands.entity(entity).insert(MissileVelocity(velocity));
    if let Ok(client_pos) = missile_data.get(entity) {
        commands
            .entity(entity)
            .insert(ServerReconciliation::new(*client_pos, movement.pos, velocity, rtt));
    }
}
