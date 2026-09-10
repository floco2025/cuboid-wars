use bevy::prelude::*;
use common::{physics::MuzzleCheck, protocol::SProjectileShot};

use super::context::ServerMessageContext;
use crate::{audio::play_spatial_sound, projectiles::spawn_projectiles};

pub(super) fn handle_projectile_shot_message(
    message: SProjectileShot,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    if message.id == context.my_player_id.0 {
        return;
    }
    if spawn_projectiles(
        commands,
        &context.assets.projectile_assets,
        &message.shot,
        &context.gameplay_config,
        context.map_settings.movement.projectile_speed,
        &context.collision_world,
        &context.plates.open_barrier_kinds,
        message.id,
        MuzzleCheck::Skipped,
    ) > 0
    {
        play_spatial_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("fire"),
            &context.client_settings.audio,
            message.shot.origin.into(),
        );
    }
}
