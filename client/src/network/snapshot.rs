use bevy::prelude::*;
use common::protocol::*;

use crate::barriers::{LockedPlatePurposes, OpenBarrierKinds};

use super::{
    actors::{sync_actors, sync_spawning_actors},
    context::ServerMessageContext,
    items::sync_items,
    missiles::sync_missiles,
    players::sync_players,
    portals::sync_portals,
};

pub(super) fn handle_snapshot_message(
    message: SSnapshot,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if !sequence_is_newer(message.seq, context.last_snapshot_seq.0) {
        warn!(
            "ignoring an outdated snapshot (seq {}, last {})",
            message.seq, context.last_snapshot_seq.0
        );
        return;
    }
    context.last_snapshot_seq.0 = message.seq;

    // Avoid marking an untouched quest log as changed on every snapshot.
    if !message.quests.is_empty() {
        context.quest_log.apply_group_status(&message.quests);
    }

    sync_players(commands, context, my_player_id, &message.players);
    sync_actors(commands, context, &message.actors);
    sync_spawning_actors(commands, context, &message.spawning_actors);
    sync_items(commands, context, &message.items);
    sync_missiles(commands, context, &message.missiles);
    sync_portals(commands, context, &message.portals);

    // Stable equality keeps identical snapshots from waking the visibility systems.
    context
        .open_barrier_kinds
        .set_if_neq(OpenBarrierKinds(message.open_barrier_kinds));
    context
        .locked_plate_purposes
        .set_if_neq(LockedPlatePurposes(message.locked_plate_purposes));

    context.rain_intensity.target = message.rain_intensity;
    if context.lighting.target != message.lighting {
        context.lighting.target = message.lighting;
    }
    context.lighting.synced = true;
}
