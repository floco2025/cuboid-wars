use bevy::prelude::*;
use common::protocol::*;

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
    if !context.last_snapshot_seq.should_accept(message.seq) {
        warn!(
            "Ignoring outdated SSnapshot (seq: {}, last: {})",
            message.seq,
            context
                .last_snapshot_seq
                .last_raw()
                .map_or_else(|| "none".to_string(), |seq| seq.to_string())
        );
        return;
    }

    context.last_snapshot_seq.record(message.seq);

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

    // The server sorts these vectors, so equality is stable across snapshots.
    if context.open_barrier_kinds.0 != message.open_barrier_kinds {
        context.open_barrier_kinds.0 = message.open_barrier_kinds;
    }
    if context.locked_plate_purposes.0 != message.locked_plate_purposes {
        context.locked_plate_purposes.0 = message.locked_plate_purposes;
    }

    context.rain_intensity.target = message.rain_intensity;
    if context.lighting.target != message.lighting {
        context.lighting.target = message.lighting;
    }
    context.lighting.synced = true;
}
