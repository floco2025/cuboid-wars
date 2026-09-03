use bevy::prelude::*;
use common::protocol::{SFeed, SFirework, SPressurePlate};

use super::context::ServerMessageContext;
use crate::audio::play_sound;

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
    play_sound(commands, &context.asset_server, context.asset_set.player_sound(sound));
}

pub(super) fn handle_firework_message(message: SFirework, context: &mut ServerMessageContext) {
    context.firework_show.start(message.seed, Some(&context.map_layout));
}

pub(super) fn handle_feed_message(message: SFeed, context: &mut ServerMessageContext) {
    context.feed.push(message);
}
