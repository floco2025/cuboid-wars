use super::super::context::ServerMessageContext;
use crate::{
    characters::PreviousTickPosition,
    network::resources::accept_newer_tick,
    players::{PlayerInfo, RemotePlayerMotion},
};
use bevy::prelude::*;
use common::{map::Carriers, physics::PlayerMotionBundle, protocol::*};

pub(in crate::network) fn handle_player_moves_message(
    message: SPlayerMoves,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if !accept_newer_tick(&mut context.clocks.last_player_moves_tick.0, message.tick) {
        return;
    }
    let tick = message.tick;
    if context.clocks.tick_sync.takes_rough_seed() {
        context.clocks.server_tick.0 = tick.wrapping_add(1);
    }
    for entry in message.moves {
        let Some(player) = context.players.get_mut(&entry.id) else {
            continue;
        };
        if entry.id == my_player_id
            || entry.generation != player.generation
            || !sequence_is_newer(tick, player.last_movement_tick)
        {
            continue;
        }
        player.last_movement_tick = tick;
        let delay = context.client_settings.interpolation.delay_ticks(&context.network);
        commands.entity(player.entity).queue(move |mut entity: EntityWorldMut| {
            if let Some(mut motion) = entity.get_mut::<RemotePlayerMotion>() {
                motion.push(entry, delay);
            }
        });
    }
}

pub(in crate::network) fn snap_player(
    commands: &mut Commands,
    info: &PlayerInfo,
    movement: &PlayerMovementState,
    carriers: &Carriers,
) {
    let pos = carriers.pose(movement.carrier).transform_position(&movement.pos);
    commands
        .entity(info.entity)
        .insert((pos, PreviousTickPosition(pos), PlayerMotionBundle::from(movement)));
}
