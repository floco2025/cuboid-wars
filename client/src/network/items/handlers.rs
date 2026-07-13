use bevy::prelude::*;

use crate::{config::AssetSet, players::PlayerMap};
use common::protocol::*;

// ============================================================================
// Item Message Handlers
// ============================================================================

// Cookie pickup: play sound + apply the early score for HUD reaction. The
// snapshot will confirm `score` next tick; this is just the latency cut.
pub fn handle_item_collected_message(
    commands: &mut Commands,
    msg: SCookieCollected,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    players: &mut PlayerMap,
    my_player_id: PlayerId,
) {
    if let Some(info) = players.get_mut(&my_player_id) {
        info.score = msg.score;
    }
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.player_sound("collect_cookie").to_owned())),
        PlaybackSettings::DESPAWN,
    ));
}

// Health potion pickup: play sound + apply the early Health for the HUD bar.
// The snapshot will confirm `Health` next tick; this is just the latency cut.
pub fn handle_health_potion_collected_message(
    commands: &mut Commands,
    msg: SHealthPotionCollected,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    players: &PlayerMap,
    my_player_id: PlayerId,
) {
    if let Some(info) = players.get(&my_player_id) {
        commands.entity(info.entity).insert(msg.health);
    }
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.player_sound("collect_power_up").to_owned())),
        PlaybackSettings::DESPAWN,
    ));
}
