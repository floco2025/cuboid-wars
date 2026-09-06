use bevy::prelude::*;

use super::{super::context::ServerMessageContext, sync::apply_missile_movement_state};
use crate::{
    audio::{play_explosion_sound, play_sound, play_spatial_sound},
    missiles::spawn_missile,
    vfx::spawn_missile_explosion,
};
use common::protocol::*;

// A missile launched. Spawn it now — clients don't predict missile spawns,
// so this cue is the first sight of it; a racing snapshot may already have
// spawned it (stay idempotent). The launch sound plays here for everyone —
// flat for the shooter, spatial for bystanders — so a server-rejected shot
// never leaves an orphaned sound.
pub(in crate::network) fn handle_missile_launch_message(
    message: SMissileLaunch,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if !context.missiles.contains_key(&message.id) {
        let entity = spawn_missile(commands, &context.missile_assets, message.id, &message.movement);
        context.missiles.insert(message.id, entity);
    }
    if message.shooter == my_player_id {
        play_sound(
            commands,
            &context.asset_server,
            context.asset_set.player_sound("missile_launch"),
        );
    } else {
        play_spatial_sound(
            commands,
            &context.asset_server,
            context.asset_set.player_sound("missile_launch"),
            &context.client_settings.audio,
            Vec3::from(message.movement.pos),
        );
    }
}

pub(in crate::network) fn handle_missile_move_message(
    message: SMissileMove,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    apply_missile_movement_state(
        commands,
        &context.missiles,
        &context.rtt,
        &context.missile_data,
        message.id,
        message.movement,
    );
}

// Detonation: explosion VFX + spatial boom at the server's impact point,
// then teardown. Idempotent against the snapshot diff having already
// despawned the entity.
pub(in crate::network) fn handle_missile_detonated_message(
    message: SMissileDetonated,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    let Some(entity) = context.missiles.remove(&message.id) else {
        return;
    };
    spawn_missile_explosion(commands, &mut context.explosion_ctx(), message.pos);
    play_explosion_sound(
        commands,
        &context.asset_server,
        context.asset_set.player_sound("explodes"),
        &context.client_settings.audio,
        Vec3::from(message.pos),
        Some(context.blast_radii.missile),
    );
    commands.entity(entity).despawn();
}

// Missile pack pickup: sound + the early ammo count for the HUD. The
// snapshot's `Player.missiles` is the system of record.
pub(in crate::network) fn handle_missiles_collected_message(
    message: SMissilesCollected,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if let Some(info) = context.players.get_mut(&my_player_id) {
        info.missiles = message.missiles;
        context.pending_weapon_selection.collect(ItemType::MissilePack);
    }
    play_sound(
        commands,
        &context.asset_server,
        context.asset_set.player_sound("collect_power_up"),
    );
}
