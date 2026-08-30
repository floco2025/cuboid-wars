use bevy::{ecs::system::SystemParam, prelude::*};

use super::sync::apply_missile_movement_state;
use crate::{
    audio::{play_explosion_sound, play_sound, play_spatial_sound},
    config::{AssetSet, AudioConfig, ClientSettings},
    missiles::{MissileAssets, MissileMap, spawn_missile},
    network::RoundTripTime,
    players::PlayerMap,
    vfx::{BlastRadii, ExplosionAssets, ExplosionSpawnCtx, ExplosionVfxBudget, spawn_missile_explosion},
};
use common::{config::GameplayConfig, physics::CollisionWorld, protocol::*};

#[derive(SystemParam)]
pub(in crate::network) struct MissileMessageContext<'w, 's> {
    missile_assets: Res<'w, MissileAssets>,
    missiles: ResMut<'w, MissileMap>,
    asset_server: Res<'w, AssetServer>,
    asset_set: Res<'w, AssetSet>,
    client_settings: Res<'w, ClientSettings>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    explosion_vfx_budget: ResMut<'w, ExplosionVfxBudget>,
    explosion_assets: Res<'w, ExplosionAssets>,
    gameplay_config: Res<'w, GameplayConfig>,
    collision_world: Option<Res<'w, CollisionWorld>>,
    map_layout: Option<Res<'w, MapLayout>>,
    blast_radii: Res<'w, BlastRadii>,
    missile_data: Query<'w, 's, &'static Position, With<MissileMarker>>,
    players: ResMut<'w, PlayerMap>,
}

pub(in crate::network) fn handle_missile_launch_message(
    message: &SMissileLaunch,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut MissileMessageContext,
) {
    apply_missile_launch(
        commands,
        &context.missile_assets,
        &mut context.missiles,
        &context.asset_server,
        &context.asset_set,
        &context.client_settings.audio,
        my_player_id,
        message,
    );
}

pub(in crate::network) fn handle_missile_move_message(
    message: &SMissileMove,
    commands: &mut Commands,
    rtt: &RoundTripTime,
    context: &mut MissileMessageContext,
) {
    apply_missile_move(commands, &context.missiles, rtt, &context.missile_data, message);
}

pub(in crate::network) fn handle_missile_death_message(
    message: &SMissileDeath,
    commands: &mut Commands,
    context: &mut MissileMessageContext,
) {
    let mut ctx = ExplosionSpawnCtx {
        meshes: &mut context.meshes,
        materials: &mut context.materials,
        budget: &mut context.explosion_vfx_budget,
        explosion_assets: &context.explosion_assets,
        gameplay_config: &context.gameplay_config,
        collision_world: context.collision_world.as_deref(),
        map_layout: context.map_layout.as_deref(),
    };
    apply_missile_death(
        commands,
        &mut ctx,
        &context.blast_radii,
        &context.asset_server,
        &context.asset_set,
        &context.client_settings.audio,
        &mut context.missiles,
        message,
    );
}

pub(in crate::network) fn handle_missiles_collected_message(
    message: &SMissilesCollected,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut MissileMessageContext,
) {
    apply_missiles_collected(
        commands,
        message,
        &context.asset_server,
        &context.asset_set,
        &mut context.players,
        my_player_id,
    );
}

// A missile launched. Spawn it now — clients don't predict missile spawns,
// so this cue is the first sight of it; a racing snapshot may already have
// spawned it (stay idempotent). The launch sound plays here for everyone —
// flat for the shooter, spatial for bystanders — so a server-rejected shot
// never leaves an orphaned sound.
#[expect(
    clippy::too_many_arguments,
    reason = "launch presentation dependencies stay explicit"
)]
fn apply_missile_launch(
    commands: &mut Commands,
    missile_assets: &MissileAssets,
    missiles: &mut MissileMap,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    my_player_id: PlayerId,
    event: &SMissileLaunch,
) {
    if !missiles.contains_key(&event.id) {
        let entity = spawn_missile(commands, missile_assets, event.id, &event.movement);
        missiles.insert(event.id, entity);
    }
    if event.shooter == my_player_id {
        play_sound(commands, asset_server, asset_set.player_sound("missile_launch"));
    } else {
        play_spatial_sound(
            commands,
            asset_server,
            asset_set.player_sound("missile_launch"),
            audio_config,
            Vec3::from(event.movement.pos),
        );
    }
}

fn apply_missile_move(
    commands: &mut Commands,
    missiles: &MissileMap,
    rtt: &RoundTripTime,
    missile_data: &Query<&Position, With<MissileMarker>>,
    event: &SMissileMove,
) {
    apply_missile_movement_state(commands, missiles, rtt, missile_data, event.id, event.movement);
}

// Detonation: explosion VFX + spatial boom at the server's impact point,
// then teardown. Idempotent against the snapshot diff having already
// despawned the entity.
fn apply_missile_death(
    commands: &mut Commands,
    ctx: &mut ExplosionSpawnCtx,
    blast_radii: &BlastRadii,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    missiles: &mut MissileMap,
    event: &SMissileDeath,
) {
    let Some(entity) = missiles.remove(&event.id) else {
        return;
    };
    spawn_missile_explosion(commands, ctx, blast_radii, event.pos);
    play_explosion_sound(
        commands,
        asset_server,
        asset_set.player_sound("explodes"),
        audio_config,
        Vec3::from(event.pos),
        Some(blast_radii.missile),
    );
    commands.entity(entity).despawn();
}

// Missile pack pickup: sound + the early ammo count for the HUD. The
// snapshot's `Player.missiles` is the system of record.
fn apply_missiles_collected(
    commands: &mut Commands,
    event: &SMissilesCollected,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    players: &mut PlayerMap,
    my_player_id: PlayerId,
) {
    if let Some(info) = players.get_mut(&my_player_id) {
        info.missiles = event.missiles;
    }
    play_sound(commands, asset_server, asset_set.player_sound("collect_power_up"));
}
