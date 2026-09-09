use bevy::prelude::*;

use super::{super::context::ServerMessageContext, sync::upsert_portal};
use crate::{
    audio::{play_sound, play_spatial_sound},
    players::eye_position,
    vfx::spawn_portal_fizzle,
};
use common::{physics::PortalSet, protocol::*};

// A portal end was placed or moved. The visual is idempotent against a racing
// snapshot; the cue still plays the accepted shot's gun sound exactly once.
pub(in crate::network) fn handle_portal_opened_message(
    message: SPortalOpened,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if upsert_portal(commands, context, &message.portal) {
        *context.portal_set = PortalSet::rebuild(
            &context.portals.wire_portals(),
            &context.collision_world,
            &context.carriers,
        );
    }
    if message.shooter == my_player_id {
        play_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("portal_fire"),
        );
    } else if let Some(shooter) = context.players.get(&message.shooter)
        && let Ok(position) = context.player_data.get(shooter.entity)
    {
        play_spatial_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("portal_fire"),
            &context.client_settings.audio,
            eye_position(*position, context.gameplay_config.player.eye_height()),
        );
    }
}

pub(in crate::network) fn handle_portal_fizzled_message(
    message: SPortalFizzled,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    spawn_portal_fizzle(
        commands,
        &context.assets.portal_fizzle_assets,
        &message.impact,
        &context.carrier_entities,
    );
    if message.shooter == my_player_id {
        play_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("portal_fire"),
        );
        play_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("portal_fizzle"),
        );
    } else {
        let impact = context
            .carriers
            .pose(message.impact.carrier)
            .transform_point(message.impact.pos.into());
        play_spatial_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("portal_fizzle"),
            &context.client_settings.audio,
            impact,
        );
        if let Some(shooter) = context.players.get(&message.shooter)
            && let Ok(position) = context.player_data.get(shooter.entity)
        {
            play_spatial_sound(
                commands,
                &context.assets.asset_server,
                context.assets.asset_set.player_sound("portal_fire"),
                &context.client_settings.audio,
                eye_position(*position, context.gameplay_config.player.eye_height()),
            );
        }
    }
}
