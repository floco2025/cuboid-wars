use bevy::prelude::*;
use std::collections::HashSet;

use super::super::{
    components::{ServerReconciliation, extrapolated_correction},
    context::ServerMessageContext,
};
use crate::{
    missiles::{MissileMap, MissileVelocity, spawn_missile},
    network::RoundTripTime,
};
use common::protocol::{Missile, MissileId, MissileMarker, MissileMovementState, Position};

// Snapshot diff for missiles, the same idiom as `sync_actors`: spawn ids the
// snapshot has and we don't, silently despawn ids it dropped (the
// `SMissileDetonated` cue owns the detonation VFX), then apply the carried
// movement as a reconciliation target.
pub(in crate::network) fn sync_missiles(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    server_missiles: &[(MissileId, Missile)],
) {
    let update_ids: HashSet<MissileId> = server_missiles.iter().map(|(id, _)| *id).collect();

    for (id, missile) in server_missiles {
        if context.missiles.contains_key(id) {
            continue;
        }
        let entity = spawn_missile(commands, &context.missile_assets, *id, &missile.movement);
        context.missiles.insert(*id, entity);
    }

    context.missiles.retain(|id, entity| {
        if update_ids.contains(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    for (id, missile) in server_missiles {
        apply_missile_movement_state(
            commands,
            &context.missiles,
            &context.rtt,
            &context.missile_data,
            *id,
            missile.movement,
        );
    }
}

pub(super) fn apply_missile_movement_state(
    commands: &mut Commands,
    missiles: &MissileMap,
    rtt: &RoundTripTime,
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
        commands.entity(entity).insert(ServerReconciliation::new(
            extrapolated_correction(*client_pos, movement.pos, velocity, rtt),
            movement.pos,
            velocity,
            rtt,
        ));
    }
}
