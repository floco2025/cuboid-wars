use bevy::prelude::*;

use super::sync::apply_missile_movement_state;
use crate::{
    audio::{play_explosion_sound, play_sound, play_spatial_sound},
    config::{AssetSet, AudioConfig},
    missiles::{MissileAssets, MissileMap, spawn_missile},
    network::RoundTripTime,
    players::PlayerMap,
    vfx::{ExplosionSpawnCtx, spawn_missile_explosion},
};
use common::protocol::{
    MissileMarker, PlayerId, Position, SMissileDeath, SMissileLaunch, SMissileMove, SMissilesCollected,
};

// A missile launched. Spawn it now — clients don't predict missile spawns,
// so this cue is the first sight of it; a racing snapshot may already have
// spawned it (stay idempotent). The launch sound plays here for everyone —
// flat for the shooter, spatial for bystanders — so a server-rejected shot
// never leaves an orphaned sound.
#[expect(clippy::too_many_arguments, reason = "message handler threading dispatcher state")]
pub fn handle_missile_launch_message(
    commands: &mut Commands,
    missile_assets: &MissileAssets,
    missiles: &mut MissileMap,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    my_player_id: PlayerId,
    msg: SMissileLaunch,
) {
    if !missiles.contains_key(&msg.id) {
        let entity = spawn_missile(commands, missile_assets, msg.id, &msg.movement);
        missiles.insert(msg.id, entity);
    }
    if msg.shooter == my_player_id {
        play_sound(commands, asset_server, asset_set.player_sound("missile_launch"));
    } else {
        play_spatial_sound(
            commands,
            asset_server,
            asset_set.player_sound("missile_launch"),
            audio_config,
            Vec3::from(msg.movement.pos),
        );
    }
}

pub fn handle_missile_move_intent_message(
    commands: &mut Commands,
    missiles: &MissileMap,
    rtt: &RoundTripTime,
    missile_data: &Query<&Position, With<MissileMarker>>,
    msg: SMissileMove,
) {
    apply_missile_movement_state(commands, missiles, rtt, missile_data, msg.id, msg.movement);
}

// Detonation: explosion VFX + spatial boom at the server's impact point,
// then teardown. Idempotent against the snapshot diff having already
// despawned the entity.
pub fn handle_missile_death_message(
    commands: &mut Commands,
    ctx: &mut ExplosionSpawnCtx,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    missiles: &mut MissileMap,
    msg: SMissileDeath,
) {
    let Some(entity) = missiles.remove(&msg.id) else {
        return;
    };
    spawn_missile_explosion(commands, ctx, msg.pos);
    play_explosion_sound(
        commands,
        asset_server,
        asset_set.player_sound("explodes"),
        audio_config,
        Vec3::from(msg.pos),
        Some(ctx.gameplay_config.missiles.blast_radius),
    );
    commands.entity(entity).despawn();
}

// Missile pack pickup: sound + the early ammo count for the HUD. The
// snapshot's `Player.missiles` is the system of record.
pub fn handle_missiles_collected_message(
    commands: &mut Commands,
    msg: SMissilesCollected,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    players: &mut PlayerMap,
    my_player_id: PlayerId,
) {
    if let Some(info) = players.get_mut(&my_player_id) {
        info.missiles = msg.missiles;
    }
    play_sound(commands, asset_server, asset_set.player_sound("collect_power_up"));
}
