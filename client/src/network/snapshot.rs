use bevy::prelude::*;
use common::protocol::*;

use crate::barriers::LockedPlatePurposes;

use super::{
    actors::{sync_actors, sync_spawning_actors},
    context::ServerMessageContext,
    items::sync_items,
    missiles::sync_missiles,
    players::sync_players,
    portals::sync_portals,
    resources::accept_newer_tick,
};

pub(super) fn handle_snapshot_message(
    message: SSnapshot,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if !accept_newer_tick(&mut context.last_snapshot_tick.0, message.tick) {
        warn!(
            "ignoring an outdated snapshot (tick {}, last {:?})",
            message.tick, context.last_snapshot_tick.0
        );
        return;
    }
    if context.tick_sync.takes_rough_seed() {
        context.server_tick.0 = message.tick.wrapping_add(1);
    }

    // Avoid marking an untouched quest log as changed on every snapshot.
    if !message.quests.is_empty() {
        context.quest_log.apply_group_status(&message.quests);
    }

    sync_players(commands, context, my_player_id, message.tick, &message.players);
    sync_actors(commands, context, &message.actors);
    sync_spawning_actors(commands, context, &message.spawning_actors);
    sync_items(commands, context, &message.items);
    sync_missiles(commands, context, &message.missiles);
    sync_portals(commands, context, &message.portals);

    // Stable equality keeps identical snapshots from waking the visibility systems.
    context.plates.set_if_neq(message.plates);
    context
        .locked_plate_purposes
        .set_if_neq(LockedPlatePurposes(message.locked_plate_purposes));

    context.rain_intensity.target = message.rain_intensity;
    if context.lighting.target != message.lighting {
        context.lighting.target = message.lighting;
    }
    context.lighting.synced = true;
}
