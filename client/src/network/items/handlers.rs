use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{audio::play_sound, config::AssetSet, players::PlayerMap};
use common::protocol::*;

#[derive(SystemParam)]
pub(in crate::network) struct ItemMessageContext<'w> {
    asset_server: Res<'w, AssetServer>,
    asset_set: Res<'w, AssetSet>,
    players: ResMut<'w, PlayerMap>,
}

pub(in crate::network) fn handle_cookie_collected_message(
    message: &SCookieCollected,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ItemMessageContext,
) {
    apply_cookie_collected(
        commands,
        message,
        &context.asset_server,
        &context.asset_set,
        &mut context.players,
        my_player_id,
    );
}

pub(in crate::network) fn handle_health_potion_collected_message(
    message: &SHealthPotionCollected,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ItemMessageContext,
) {
    apply_health_potion_collected(
        commands,
        message,
        &context.asset_server,
        &context.asset_set,
        &context.players,
        my_player_id,
    );
}

// Cookie pickup: play sound + apply the early score for HUD reaction. The
// snapshot will confirm `score` next tick; this is just the latency cut.
fn apply_cookie_collected(
    commands: &mut Commands,
    event: &SCookieCollected,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    players: &mut PlayerMap,
    my_player_id: PlayerId,
) {
    if let Some(info) = players.get_mut(&my_player_id) {
        info.score = event.score;
    }
    play_sound(commands, asset_server, asset_set.player_sound("collect_cookie"));
}

// Health potion pickup: play sound + apply the early Health for the HUD bar.
// The snapshot will confirm `Health` next tick; this is just the latency cut.
fn apply_health_potion_collected(
    commands: &mut Commands,
    event: &SHealthPotionCollected,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    players: &PlayerMap,
    my_player_id: PlayerId,
) {
    if let Some(info) = players.get(&my_player_id) {
        commands.entity(info.entity).insert(event.health);
    }
    play_sound(commands, asset_server, asset_set.player_sound("collect_power_up"));
}
