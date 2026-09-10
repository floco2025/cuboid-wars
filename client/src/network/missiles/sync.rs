use super::super::context::ServerMessageContext;
use crate::{
    missiles::{MissileInfo, MissileMap, RemoteMissileMotion, spawn_missile},
    network::SampleTiming,
};
use bevy::prelude::*;
use common::protocol::*;
use std::collections::HashSet;

pub(in crate::network) fn sync_missiles(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    tick: u32,
    server_missiles: &[(MissileId, Missile)],
) {
    let my_player_id = context.my_player_id.0;
    context.missiles.discard_retired_before(tick);
    let listed: HashSet<_> = server_missiles.iter().map(|(id, _)| *id).collect();
    for (id, missile) in server_missiles {
        // Only the reliable launch starts the owner; snapshots never restart or reposition its flight.
        if missile.shooter == my_player_id || context.missiles.is_retired(id) {
            continue;
        }
        match context.missiles.get(id) {
            Some(info) => apply_missile_movement_state(commands, info.entity, missile.seq, missile.movement),
            None => {
                let entity = spawn_missile(commands, &context.assets.missile_assets, *id, &missile.movement);
                commands.entity(entity).insert(RemoteMissileMotion::new(
                    missile.seq,
                    missile.movement,
                    SampleTiming::new(&context.client_settings.interpolation, &context.network),
                ));
                context.missiles.insert(
                    *id,
                    MissileInfo {
                        entity,
                        shooter: missile.shooter,
                        born_tick: tick,
                        impact_pending: false,
                    },
                );
            }
        }
    }
    for id in stale_missile_ids(&context.missiles, my_player_id, tick, &listed) {
        if let Some(info) = context.missiles.take(&id) {
            commands.entity(info.entity).despawn();
        }
    }
}

// Observed missiles the snapshot no longer lists, except those launched after
// the tick it describes and those still flying out to a reported impact.
fn stale_missile_ids(
    missiles: &MissileMap,
    my_player_id: PlayerId,
    tick: u32,
    listed: &HashSet<MissileId>,
) -> Vec<MissileId> {
    missiles
        .iter()
        .filter(|(id, info)| {
            info.shooter != my_player_id
                && !info.impact_pending
                && !listed.contains(id)
                && !sequence_is_newer(info.born_tick, tick)
        })
        .map(|(id, _)| *id)
        .collect()
}

pub(super) fn apply_missile_movement_state(
    commands: &mut Commands,
    entity: Entity,
    seq: u32,
    movement: MissileMovementState,
) {
    commands.entity(entity).queue(move |mut entity: EntityWorldMut| {
        if let Some(mut motion) = entity.get_mut::<RemoteMissileMotion>() {
            motion.push(seq, movement);
        }
    });
}

#[cfg(test)]
#[path = "tests/sync.rs"]
mod tests;
