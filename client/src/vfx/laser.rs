use std::time::Duration;

use bevy::{audio::SpatialScale, light::NotShadowCaster, prelude::*};

use crate::{
    actors::{ActorMap, AimJointMarker, AimRig},
    config::{AssetSet, ClientSettings},
    constants::*,
    players::PlayerMap,
};
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{ActorId, PlateState, PlayerId, Position},
};

// Angular speeds (rad/s) of the endpoint wander's per-axis sines —
// incommensurate so the combined path never visibly repeats, ~1–1.5 Hz so
// the drift reads as searching, not strobing.
const WANDER_SPEEDS: Vec3 = Vec3::new(7.3, 9.4, 5.1);
// Golden angle: spreads per-beam phases so simultaneous beams desync.
const WANDER_PHASE_STEP: f32 = 2.399;

#[derive(Component)]
pub struct LaserBeam {
    pub actor: ActorId,
    pub target: PlayerId,
    pub started_tick: u32,
    pub wander_width_fraction: f32,
    pub wander_height_fraction: f32,
    pub aim_height_fraction: f32,
}

pub fn spawn_laser_beam(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    actor: ActorId,
    target: PlayerId,
    started_tick: u32,
) -> Entity {
    let brightness = LASER_EMISSIVE;
    // Opaque hot-red emissive core, like the projectile body — this app's
    // Blend materials render wrong, so no translucency; bloom supplies the
    // glow. The base color stays fixed red — only the emissive scales with
    // the brightness knob, so cranking the glow can't wash the surface to
    // white. Per-instance assets free themselves when the beam despawns.
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.15, 0.15),
        emissive: LinearRgba::rgb(brightness, 0.08 * brightness, 0.08 * brightness),
        ..default()
    });
    // Unit-height cylinder with the real radius baked in; the update system
    // scales Y to the live beam length. Returned so the caller can attach
    // beam-lifetime extras (the fire sound) that must stop when the
    // beam despawns.
    commands
        .spawn((
            LaserBeam {
                actor,
                target,
                started_tick,
                wander_width_fraction: LASER_ENDPOINT_WANDER_WIDTH_FRACTION,
                wander_height_fraction: LASER_ENDPOINT_WANDER_HEIGHT_FRACTION,
                aim_height_fraction: LASER_AIM_HEIGHT_FRACTION,
            },
            Mesh3d(meshes.add(Cylinder::new(LASER_BEAM_RADIUS, 1.0))),
            MeshMaterial3d(material),
            NotShadowCaster,
            Transform::default(),
            // Hidden until the first update frame anchors it — the default
            // transform would otherwise flash at the world origin.
            Visibility::Hidden,
        ))
        .id()
}

// Beam anchors use this frame's interpolated character positions.
pub fn laser_beam_update_system(
    mut commands: Commands,
    time: Res<Time>,
    actors: Res<ActorMap>,
    players: Res<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    collision_world: Res<CollisionWorld>,
    plates: Res<PlateState>,
    endpoints: Query<(&Transform, Option<&AimRig>), (Without<LaserBeam>, Without<AimJointMarker>)>,
    mut joints: Query<&mut Transform, With<AimJointMarker>>,
    mut beams: Query<(Entity, &LaserBeam, &mut Transform, &mut Visibility), Without<AimJointMarker>>,
) {
    // The target's configured bounding box: the beam anchors at its center
    // (the player root transform) and the wander stays inside the
    // `wander_fraction`-scaled box.
    let target_hitbox = gameplay_config.player.physics().hitbox;
    let target_half_extents = Vec3::new(
        target_hitbox.width / 2.0,
        target_hitbox.height / 2.0,
        target_hitbox.depth / 2.0,
    );
    for (entity, beam, mut transform, mut visibility) in &mut beams {
        let anchors = actors
            .get(&beam.actor)
            .zip(players.get(&beam.target))
            .and_then(|(actor, target)| {
                Some((
                    endpoints.get(actor.entity).ok()?,
                    endpoints.get(target.entity).ok()?,
                    gameplay_config.expect_actor(&actor.kind),
                ))
            });
        let Some(((actor_transform, aim_rig), (target_transform, _), actor_config)) = anchors else {
            commands.entity(entity).despawn();
            continue;
        };
        let frame = aim_rig.and_then(|rig| {
            rig.frame(actor_transform, |entity| {
                endpoints.get(entity).ok().map(|(transform, _)| *transform)
            })
        });
        if aim_rig.is_some() && frame.is_none() {
            *visibility = Visibility::Hidden;
            continue;
        }
        let articulated = aim_rig.zip(frame.as_ref());
        let origin = articulated.map_or_else(
            || actor_transform.translation + Vec3::Y * actor_config.beam_origin_height(),
            |(rig, frame)| rig.pivot(frame),
        );
        // Drift the hit point smoothly around the target's box center so the
        // beam reads as searching rather than pinned. Sized per axis from the
        // configured collider box and rotated into the target's frame, so the
        // drift stays inside the fraction-scaled box (width and depth differ).
        // Applied before the length computation and the wall clip so a
        // blocked beam wanders too. Damage is unaffected — the server burns
        // the box center.
        let phase = beam.actor.0 as f32 * WANDER_PHASE_STEP;
        let elapsed = time.elapsed_secs();
        let wander_local = Vec3::new(
            (elapsed * WANDER_SPEEDS.x + phase).sin(),
            (elapsed * WANDER_SPEEDS.y + 2.0 * phase).sin(),
            (elapsed * WANDER_SPEEDS.z + 3.0 * phase).sin(),
        ) * target_half_extents
            * Vec3::new(
                beam.wander_width_fraction,
                beam.wander_height_fraction,
                beam.wander_width_fraction,
            );
        // The anchor sits `aim_height_fraction` up the hitbox (0.5 = center).
        let anchor_y = target_hitbox.bottom_offset + beam.aim_height_fraction * target_hitbox.height;
        let aim_local = Vec3::new(0.0, anchor_y, 0.0) + wander_local;
        let target = target_transform.translation + target_transform.rotation * aim_local;
        let full_length = origin.distance(target);
        if full_length <= f32::EPSILON {
            *visibility = Visibility::Hidden;
            continue;
        }
        let direction = (target - origin) / full_length;
        if let Some((rig, frame)) = articulated {
            rig.aim(frame, direction, &mut joints);
        }
        // Damage and beam clipping share the active-field filter.
        let length = collision_world
            .attack_surface_along_ray(origin, direction, full_length, &plates.open_barrier_kinds)
            .map_or(full_length, |hit| hit.point.distance(origin));
        // Clip from the pivot so a muzzle extending through cover cannot bypass it.
        let muzzle_distance = articulated.map_or(0.0, |(rig, frame)| rig.muzzle_distance(frame));
        if length <= muzzle_distance {
            *visibility = Visibility::Hidden;
            continue;
        }
        transform.translation = origin + direction * ((length + muzzle_distance) / 2.0);
        transform.rotation = Quat::from_rotation_arc(Vec3::Y, direction);
        transform.scale = Vec3::new(1.0, length - muzzle_distance, 1.0);
        *visibility = Visibility::Visible;
    }
}

pub fn attach_laser_audio(
    commands: &mut Commands,
    beam: Entity,
    kind: &str,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    settings: &ClientSettings,
    pos: Option<Position>,
    elapsed_secs: f32,
) {
    let mut entity = commands.entity(beam);
    entity.insert((
        AudioPlayer::new(asset_server.load(asset_set.actor_sound(kind, "fire").to_owned())),
        PlaybackSettings::ONCE
            .with_start_position(Duration::from_secs_f32(elapsed_secs))
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(settings.audio.spatial_distance_scale)),
    ));
    if let Some(pos) = pos {
        entity.insert(Transform::from_translation(Vec3::from(pos)));
    }
}
