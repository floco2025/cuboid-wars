use bevy::prelude::*;

use super::super::context::ServerMessageContext;
use crate::audio::play_sound;
use common::protocol::*;

// Cookie pickup: play sound + apply the early score for HUD reaction. The
// snapshot will confirm `score` next tick; this is just the latency cut.
pub(in crate::network) fn handle_cookie_collected_message(
    message: SCookieCollected,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if let Some(info) = context.players.get_mut(&my_player_id) {
        info.score = message.score;
    }
    play_sound(
        commands,
        &context.asset_server,
        context.asset_set.player_sound("collect_cookie"),
    );
}

// Health potion pickup: play sound + apply the early Health for the HUD bar.
// The snapshot will confirm `Health` next tick; this is just the latency cut.
pub(in crate::network) fn handle_health_potion_collected_message(
    message: SHealthPotionCollected,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if let Some(info) = context.players.get(&my_player_id) {
        commands.entity(info.entity).insert(message.health);
    }
    play_sound(
        commands,
        &context.asset_server,
        context.asset_set.player_sound("collect_power_up"),
    );
}
