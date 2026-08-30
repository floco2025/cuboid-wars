use bevy::{ecs::system::SystemParam, prelude::*};
use common::protocol::{MapLayout, SFeed, SFirework, SPressurePlate};

use crate::{audio::play_sound, config::AssetSet, ui::MessageFeed, vfx::FireworkShow};

#[derive(SystemParam)]
pub(super) struct PresentationMessageContext<'w> {
    asset_server: Res<'w, AssetServer>,
    asset_set: Res<'w, AssetSet>,
    firework_show: ResMut<'w, FireworkShow>,
    map_layout: Option<Res<'w, MapLayout>>,
    feed: ResMut<'w, MessageFeed>,
}

pub(super) fn handle_pressure_plate_message(
    message: &SPressurePlate,
    commands: &mut Commands,
    context: &mut PresentationMessageContext,
) {
    let sound = if message.pressed {
        "plate_press"
    } else {
        "plate_release"
    };
    play_sound(commands, &context.asset_server, context.asset_set.player_sound(sound));
}

pub(super) fn handle_firework_message(message: &SFirework, context: &mut PresentationMessageContext) {
    context.firework_show.start(message.seed, context.map_layout.as_deref());
}

pub(super) fn handle_feed_message(message: &SFeed, context: &mut PresentationMessageContext) {
    context.feed.push(message.clone());
}
