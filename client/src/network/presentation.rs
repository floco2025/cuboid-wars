use bevy::prelude::*;
use common::protocol::{SCheckpointReached, SFeed, SFirework, SPressurePlate};

use super::context::ServerMessageContext;
use crate::{audio::play_sound, ui::BannerMessage};

pub(super) fn handle_pressure_plate_message(
    message: SPressurePlate,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    let sound = if message.pressed {
        "plate_press"
    } else {
        "plate_release"
    };
    play_sound(
        commands,
        &context.assets.asset_server,
        context.assets.asset_set.player_sound(sound),
    );
}

pub(super) fn handle_firework_message(message: SFirework, context: &mut ServerMessageContext) {
    context
        .firework_show
        .start(message.seed, Some(&context.map_layout), context.map_settings.geometry);
}

pub(super) fn handle_feed_message(message: SFeed, context: &mut ServerMessageContext) {
    context.feed.push(message);
}

pub(super) fn handle_checkpoint_reached_message(
    message: SCheckpointReached,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    let my_player_id = context.my_player_id.0;
    if let Some(info) = context.players.get_mut(&my_player_id) {
        info.apply_checkpoint(Some(message.checkpoint), message.tick);
    }
    play_sound(
        commands,
        &context.assets.asset_server,
        context.assets.asset_set.player_sound("checkpoint_reached"),
    );
    if let Some(checkpoint) = context.map_layout.checkpoints.get(usize::from(message.checkpoint)) {
        context.banner.push(BannerMessage::CheckpointReached(checkpoint.number));
    }
}
