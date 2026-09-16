use bevy::prelude::*;
use common::protocol::*;

use crate::{barriers::LockedSwitches, fields::SharedCheckpoint};

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
    if !accept_newer_tick(&mut context.clocks.last_snapshot_tick.0, message.tick) {
        warn!(
            "ignoring an outdated snapshot (tick {}, last {:?})",
            message.tick, context.clocks.last_snapshot_tick.0
        );
        return;
    }
    if context.clocks.tick_sync.takes_rough_seed() {
        context.clocks.server_tick.0 = message.tick.wrapping_add(1);
    }

    // Avoid marking an untouched quest log as changed on every snapshot.
    if !message.quests.is_empty() {
        context.quest_log.apply_group_status(&message.quests);
    }

    sync_players(commands, context, my_player_id, message.tick, &message.players);
    context.actors.peaceful = message.actors_peaceful;
    sync_actors(commands, context, message.tick, &message.actors);
    sync_spawning_actors(commands, context, &message.spawning_actors);
    sync_items(commands, context, &message.items);
    sync_missiles(commands, context, message.tick, &message.missiles);
    sync_portals(commands, context, &message.portals);

    // Stable equality keeps identical snapshots from waking the visibility systems.
    context.switch_state.set_if_neq(message.switch_state);
    context
        .locked_switches
        .set_if_neq(LockedSwitches(message.locked_switches));
    context
        .shared_checkpoint
        .set_if_neq(SharedCheckpoint(message.shared_checkpoint));

    context.weather_intensity.target = message.cloud_cover;
    context.weather_intensity.raining = message.raining;
    context.celestial_clock.set_if_neq(message.celestial_clock);
}
