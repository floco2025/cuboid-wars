use bevy::prelude::*;

use super::{super::context::ServerMessageContext, sync::apply_missile_movement_state};
use crate::{
    audio::{play_explosion_sound, play_sound, play_spatial_sound},
    missiles::{MissileFlight, MissileInfo, OwnedMissile, RemoteMissileMotion, spawn_missile},
    vfx::spawn_missile_explosion,
};
use common::protocol::*;
use rand::RngExt;
use std::f32::consts::TAU;

pub(in crate::network) fn handle_missile_launch_message(
    message: SMissileLaunch,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if context.missiles.is_retired(&message.id) {
        return;
    }
    if !context.missiles.contains_key(&message.id) {
        if message.shooter != my_player_id
            && context
                .clocks
                .last_snapshot_tick
                .0
                .is_some_and(|tick| !sequence_is_newer(message.tick, tick))
        {
            return;
        }
        let entity = spawn_missile(commands, &context.assets.missile_assets, message.id, &message.movement);
        let remote = if message.shooter == my_player_id {
            commands.entity(entity).insert(OwnedMissile {
                flight: MissileFlight::new(
                    message.shooter,
                    message.target,
                    rand::rng().random_range(0.0..TAU),
                    context.gameplay_config.missiles.lifetime_secs,
                ),
                seq: 0,
            });
            None
        } else {
            Some(RemoteMissileMotion::new(
                0,
                message.movement,
                context.client_settings.interpolation.delay_ticks(&context.network),
            ))
        };
        context.missiles.entries.insert(
            message.id,
            MissileInfo {
                entity,
                shooter: message.shooter,
                born_tick: message.tick,
                remote,
            },
        );
    }
    if message.shooter == my_player_id {
        play_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("missile_launch"),
        );
    } else {
        play_spatial_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("missile_launch"),
            &context.client_settings.audio,
            Vec3::from(message.movement.pos),
        );
    }
}

pub(in crate::network) fn handle_missile_moves_message(message: SMissileMoves, context: &mut ServerMessageContext) {
    for update in message.moves {
        apply_missile_movement_state(context, update.id, update.seq, update.movement);
    }
}

pub(in crate::network) fn handle_missile_detonated_message(
    message: SMissileDetonated,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    if !context.missiles.retire(message.id, message.tick) {
        return;
    }
    if let Some(info) = context.missiles.remove(&message.id) {
        commands.entity(info.entity).despawn();
    }
    spawn_missile_explosion(commands, &mut context.explosion_ctx(), message.pos);
    play_explosion_sound(
        commands,
        &context.assets.asset_server,
        context.assets.asset_set.player_sound("explodes"),
        &context.client_settings.audio,
        Vec3::from(message.pos),
        Some(context.assets.blast_radii.missile),
    );
}
