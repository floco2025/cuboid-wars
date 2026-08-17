use bevy::prelude::*;

use crate::{actors::ActorMap, constants::*, players::PlayerMap};
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{ActorId, PlayerId, SActorBeam},
};

// Angular speeds (rad/s) of the endpoint wander's per-axis sines —
// incommensurate so the combined path never visibly repeats, ~1–1.5 Hz so
// the drift reads as searching, not strobing.
const WANDER_SPEEDS: Vec3 = Vec3::new(7.3, 9.4, 5.1);
// Golden angle: spreads per-beam phases so simultaneous beams desync.
const WANDER_PHASE_STEP: f32 = 2.399;

// A live laser burst, anchored each frame to the interpolated actor and
// target entities — the server's beam tracks its target, so the wire carries
// only the start cue and the client derives both endpoints locally.
#[derive(Component)]
pub struct LaserBeam {
    pub actor: ActorId,
    pub target: PlayerId,
    pub remaining_secs: f32,
    pub wander_width_fraction: f32,
    pub wander_height_fraction: f32,
    pub aim_height_fraction: f32,
}

pub fn spawn_laser_beam(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    msg: &SActorBeam,
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
    // beam-lifetime extras (the looping fire sound) that must stop when the
    // beam despawns.
    commands
        .spawn((
            LaserBeam {
                actor: msg.id,
                target: msg.target,
                remaining_secs: msg.duration_secs,
                wander_width_fraction: LASER_ENDPOINT_WANDER_WIDTH_FRACTION,
                wander_height_fraction: LASER_ENDPOINT_WANDER_HEIGHT_FRACTION,
                aim_height_fraction: LASER_AIM_HEIGHT_FRACTION,
            },
            Mesh3d(meshes.add(Cylinder::new(LASER_BEAM_RADIUS, 1.0))),
            MeshMaterial3d(material),
            Transform::default(),
            // Hidden until the first update frame anchors it — the default
            // transform would otherwise flash at the world origin.
            Visibility::Hidden,
        ))
        .id()
}

// Stretch every live beam between its actor's and target's interpolated
// collider centers (the root entities' translations). Runs after both
// transform-sync systems so it reads this frame's positions.
pub fn laser_beam_update_system(
    mut commands: Commands,
    time: Res<Time>,
    actors: Res<ActorMap>,
    players: Res<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    collision_world: Option<Res<CollisionWorld>>,
    endpoints: Query<&Transform, Without<LaserBeam>>,
    mut beams: Query<(Entity, &mut LaserBeam, &mut Transform, &mut Visibility)>,
) {
    let delta = time.delta_secs();
    // The target's configured bounding box: the beam anchors at its center
    // (the player root transform) and the wander stays inside the
    // `wander_fraction`-scaled box.
    let target_collider = gameplay_config.player.physics().collider;
    let target_half_extents = Vec3::new(
        target_collider.width / 2.0,
        target_collider.height / 2.0,
        target_collider.depth / 2.0,
    );
    for (entity, mut beam, mut transform, mut visibility) in &mut beams {
        beam.remaining_secs -= delta;
        let anchors = actors
            .get(&beam.actor)
            .zip(players.get(&beam.target))
            .and_then(|(actor, target)| endpoints.get(actor.entity).ok().zip(endpoints.get(target.entity).ok()));
        // Expired, or either endpoint entity is gone (death, logoff, snapshot
        // removal) — every early end is covered without an end cue.
        let Some((actor_transform, target_transform)) = anchors.filter(|_| beam.remaining_secs > 0.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        let origin = actor_transform.translation;
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
        // The anchor sits `aim_height_fraction` up the box (0.5 = center,
        // which is what the root translation already is).
        let anchor_y = (beam.aim_height_fraction - 0.5) * target_collider.height;
        let aim_local = Vec3::new(0.0, anchor_y, 0.0) + wander_local;
        let target = target_transform.translation + target_transform.rotation * aim_local;
        let full_length = origin.distance(target);
        if full_length <= f32::EPSILON {
            *visibility = Visibility::Hidden;
            continue;
        }
        let direction = (target - origin) / full_length;
        // Clip at the first static surface so the beam doesn't pierce cover —
        // the server gates damage on the same line of sight.
        let length = collision_world
            .as_deref()
            .and_then(|world| world.world_surface_along_ray(origin, direction, full_length))
            .map_or(full_length, |hit| hit.point.distance(origin));
        transform.translation = origin + direction * (length / 2.0);
        transform.rotation = Quat::from_rotation_arc(Vec3::Y, direction);
        transform.scale = Vec3::new(1.0, length, 1.0);
        *visibility = Visibility::Visible;
    }
}
