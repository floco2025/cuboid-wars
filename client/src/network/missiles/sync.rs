use super::super::context::ServerMessageContext;
use crate::missiles::{MissileInfo, RemoteMissileMotion, spawn_missile};
use bevy::prelude::*;
use common::protocol::*;
use std::collections::HashSet;

pub(in crate::network) fn sync_missiles(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    tick: u32,
    server_missiles: &[(MissileId, Missile)],
) {
    context.missiles.discard_retired_before(tick);
    let ids: HashSet<_> = server_missiles.iter().map(|(id, _)| *id).collect();
    for (id, missile) in server_missiles {
        // Only the reliable launch starts the owner; snapshots never restart or reposition its flight.
        if missile.shooter == context.my_player_id.0 || context.missiles.is_retired(id) {
            continue;
        }
        if !context.missiles.contains_key(id) {
            let entity = spawn_missile(commands, &context.assets.missile_assets, *id, &missile.movement);
            let delay = context.client_settings.interpolation.delay_ticks(&context.network);
            context.missiles.entries.insert(
                *id,
                MissileInfo {
                    entity,
                    shooter: missile.shooter,
                    born_tick: tick,
                    remote: Some(RemoteMissileMotion::new(missile.seq, missile.movement, delay)),
                },
            );
        } else {
            apply_missile_movement_state(context, *id, missile.seq, missile.movement);
        }
    }
    context.missiles.entries.retain(|id, info| {
        if info.shooter == context.my_player_id.0 || ids.contains(id) || !sequence_is_newer(tick, info.born_tick) {
            return true;
        }
        commands.entity(info.entity).despawn();
        false
    });
}

pub(super) fn apply_missile_movement_state(
    context: &mut ServerMessageContext,
    id: MissileId,
    seq: u32,
    movement: MissileMovementState,
) {
    if let Some(motion) = context.missiles.get_mut(&id).and_then(|info| info.remote.as_mut()) {
        motion.push(seq, movement);
    }
}
