use bevy::{audio::SpatialScale, audio::Volume, prelude::*};

use super::sync::apply_missile_movement_state;
use crate::{
    config::{AssetSet, AudioConfig},
    missiles::{MissileAssets, MissileMap, spawn_missile},
    network::RoundTripTime,
    players::PlayerMap,
    vfx::{ExplosionAssets, ExplosionVfxBudget, explosion_sound_speed, spawn_missile_explosion},
};
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{
        MapLayout, MissileMarker, PlayerId, Position, SMissileDeath, SMissileLaunch, SMissileMoveIntent,
        SMissilesCollected,
    },
};

// A missile launched. Spawn it now — clients don't predict missile spawns,
// so this cue is the first sight of it; a racing snapshot may already have
// spawned it (stay idempotent). Remote launches get a spatial fire sound;
// the shooter already played theirs at send time.
#[expect(clippy::too_many_arguments, reason = "message handler threading dispatcher state")]
pub fn handle_missile_launch_message(
    commands: &mut Commands,
    missile_assets: &MissileAssets,
    missiles: &mut MissileMap,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    my_player_id: PlayerId,
    msg: SMissileLaunch,
) {
    if !missiles.contains_key(&msg.id) {
        let entity = spawn_missile(commands, missile_assets, msg.id, &msg.movement);
        missiles.insert(msg.id, entity);
    }
    if msg.shooter != my_player_id {
        commands.spawn((
            AudioPlayer::new(asset_server.load(asset_set.player_sound("fire").to_owned())),
            PlaybackSettings::DESPAWN
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale)),
            Transform::from_translation(Vec3::from(msg.movement.pos)),
        ));
    }
}

pub fn handle_missile_move_intent_message(
    commands: &mut Commands,
    missiles: &MissileMap,
    rtt: &ResMut<RoundTripTime>,
    missile_data: &Query<&Position, With<MissileMarker>>,
    msg: SMissileMoveIntent,
) {
    apply_missile_movement_state(commands, missiles, rtt, missile_data, msg.id, msg.movement);
}

// Detonation: explosion VFX + spatial boom at the server's impact point,
// then teardown. Idempotent against the snapshot diff having already
// despawned the entity.
#[expect(clippy::too_many_arguments, reason = "message handler threading dispatcher state")]
pub fn handle_missile_death_message(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    explosion_assets: &ExplosionAssets,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    gameplay_config: &GameplayConfig,
    collision_world: Option<&CollisionWorld>,
    map_layout: Option<&MapLayout>,
    missiles: &mut MissileMap,
    msg: SMissileDeath,
) {
    let Some(entity) = missiles.remove(&msg.id) else {
        return;
    };
    spawn_missile_explosion(
        commands,
        meshes,
        materials,
        budget,
        explosion_assets,
        gameplay_config,
        collision_world,
        map_layout,
        msg.pos,
    );
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.player_sound("explodes").to_owned())),
        PlaybackSettings::DESPAWN
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale))
            .with_volume(Volume::Linear(audio_config.explosion_gain_multiplier))
            .with_speed(explosion_sound_speed(gameplay_config.missiles.blast_radius)),
        Transform::from_translation(Vec3::from(msg.pos)),
    ));
    commands.entity(entity).despawn();
}

// Missile pack pickup: sound + the early ammo count for the HUD. The
// snapshot's `Player.missiles` is the system of record.
pub fn handle_missiles_collected_message(
    commands: &mut Commands,
    msg: SMissilesCollected,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    players: &mut PlayerMap,
    my_player_id: PlayerId,
) {
    if let Some(info) = players.get_mut(&my_player_id) {
        info.missiles = msg.missiles;
    }
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.player_sound("collect_power_up").to_owned())),
        PlaybackSettings::DESPAWN,
    ));
}
