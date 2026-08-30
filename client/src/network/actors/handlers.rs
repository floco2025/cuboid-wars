use bevy::{audio::SpatialScale, ecs::system::SystemParam, prelude::*};

use super::sync::apply_actor_movement_state;
use crate::{
    actors::ActorMap,
    audio::{play_explosion_sound, play_spatial_sound},
    config::{AssetSet, AudioConfig, ClientSettings},
    network::RoundTripTime,
    players::PlayerMap,
    vfx::{
        BlastRadii, ExplosionAssets, ExplosionSpawnCtx, ExplosionVfxBudget, spawn_actor_explosion, spawn_laser_beam,
    },
};
use common::{config::GameplayConfig, physics::CollisionWorld, protocol::*};

#[derive(SystemParam)]
pub(in crate::network) struct ActorMessageContext<'w, 's> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    explosion_vfx_budget: ResMut<'w, ExplosionVfxBudget>,
    explosion_assets: Res<'w, ExplosionAssets>,
    gameplay_config: Res<'w, GameplayConfig>,
    collision_world: Option<Res<'w, CollisionWorld>>,
    map_layout: Option<Res<'w, MapLayout>>,
    asset_server: Res<'w, AssetServer>,
    asset_set: Res<'w, AssetSet>,
    client_settings: Res<'w, ClientSettings>,
    blast_radii: Res<'w, BlastRadii>,
    actors: ResMut<'w, ActorMap>,
    players: ResMut<'w, PlayerMap>,
    actor_data: Query<'w, 's, (&'static Position, &'static ActorMoveIntent, &'static FaceYaw), With<ActorMarker>>,
}

pub(in crate::network) fn handle_actor_move_message(
    message: &SActorMove,
    commands: &mut Commands,
    rtt: &RoundTripTime,
    context: &mut ActorMessageContext,
) {
    apply_actor_move(commands, &context.actors, rtt, &context.actor_data, message);
}

pub(in crate::network) fn handle_actor_death_message(
    message: &SActorDeath,
    commands: &mut Commands,
    context: &mut ActorMessageContext,
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
    apply_actor_death(
        commands,
        &mut ctx,
        &context.asset_server,
        &context.asset_set,
        &context.client_settings.audio,
        &context.blast_radii,
        &mut context.actors,
        &mut context.players,
        message,
    );
}

pub(in crate::network) fn handle_actor_hit_message(
    message: &SActorHit,
    commands: &mut Commands,
    context: &mut ActorMessageContext,
) {
    apply_actor_hit(
        commands,
        &context.actors,
        &context.actor_data,
        &context.asset_server,
        &context.asset_set,
        &context.client_settings.audio,
        message,
    );
}

pub(in crate::network) fn handle_actor_beam_message(
    message: &SActorBeam,
    commands: &mut Commands,
    context: &mut ActorMessageContext,
) {
    apply_actor_beam(
        commands,
        &mut context.meshes,
        &mut context.materials,
        &context.actors,
        &context.actor_data,
        &context.asset_server,
        &context.asset_set,
        &context.client_settings.audio,
        message,
    );
}

fn apply_actor_move(
    commands: &mut Commands,
    actors: &ActorMap,
    rtt: &RoundTripTime,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceYaw), With<ActorMarker>>,
    event: &SActorMove,
) {
    apply_actor_movement_state(
        commands,
        actors,
        rtt,
        actor_data,
        event.id,
        event.movement,
        event.movement.move_intent.direction(),
    );
}

// Drives the immediate death of an actor on this client: explosion VFX +
// sound, then despawn the entity and drop the `ActorMap` entry. The
// snapshot diff is the fallback if this event was dropped.
//
// Kind isn't on the wire `SActorDeath`; we recover it from the local
// `ActorMap` where the kind was recorded when this actor was first seen.
#[expect(clippy::too_many_arguments, reason = "death presentation dependencies stay explicit")]
fn apply_actor_death(
    commands: &mut Commands,
    ctx: &mut ExplosionSpawnCtx,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    actor_blast_radii: &BlastRadii,
    actors: &mut ActorMap,
    players: &mut PlayerMap,
    event: &SActorDeath,
) {
    // Early-apply the killer's post-bonus score so the HUD bumps on the kill
    // tick. Snapshot still authoritative.
    if let (Some(killer_id), Some(killer_score)) = (event.killer, event.killer_score)
        && let Some(killer_info) = players.get_mut(&killer_id)
    {
        killer_info.score = killer_score;
    }

    let Some(info) = actors.remove(&event.id) else {
        // Already torn down (e.g. via the snapshot diff). Stay idempotent.
        return;
    };
    spawn_actor_explosion(commands, ctx, actor_blast_radii, &info.kind, event.pos);
    play_explosion_sound(
        commands,
        asset_server,
        asset_set.actor_sound(&info.kind, "explodes"),
        audio_config,
        Vec3::from(event.pos),
        actor_blast_radii.actors.get(&info.kind).copied(),
    );
    commands.entity(info.entity).despawn();
}

// Spawn the beam visual for a burst, with the firing sound looping on the
// beam entity itself — the sound follows the beam and stops the moment the
// beam despawns (burst end, or either endpoint gone), so the continuous
// loop lasts exactly as long as the beam. An unknown actor id (cue raced
// its own snapshot teardown) spawns nothing — the beam would despawn on its
// first frame anyway.
#[expect(clippy::too_many_arguments, reason = "beam presentation dependencies stay explicit")]
fn apply_actor_beam(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    actors: &ActorMap,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceYaw), With<ActorMarker>>,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    event: &SActorBeam,
) {
    let Some(info) = actors.get(&event.id) else {
        return;
    };
    let beam = spawn_laser_beam(commands, meshes, materials, event);
    let mut beam_entity = commands.entity(beam);
    beam_entity.insert((
        AudioPlayer::new(asset_server.load(asset_set.actor_sound(&info.kind, "fire").to_owned())),
        PlaybackSettings::LOOP
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale)),
    ));
    // Seed the transform at the actor so the loop's first frames are heard
    // from the right spot; the update system re-anchors it every frame.
    if let Ok((pos, _, _)) = actor_data.get(info.entity) {
        beam_entity.insert(Transform::from_translation(Vec3::from(*pos)));
    }
}

fn apply_actor_hit(
    commands: &mut Commands,
    actors: &ActorMap,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceYaw), With<ActorMarker>>,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    event: &SActorHit,
) {
    trace!("{:?} was hit", event.id);
    // Early-apply the post-hit health so the floating health bar drops on
    // the impact tick instead of waiting for the next snapshot.
    if let Some(info) = actors.get(&event.id) {
        commands.entity(info.entity).insert(event.health);
        // `SActorHit` is broadcast to every client, so the impact plays as
        // a world sound at the actor — distant fights plink faintly instead
        // of clicking at full volume map-wide.
        if let Ok((pos, _, _)) = actor_data.get(info.entity) {
            play_spatial_sound(
                commands,
                asset_server,
                asset_set.player_sound("hit_actor"),
                audio_config,
                Vec3::from(*pos),
            );
        }
    }
}
