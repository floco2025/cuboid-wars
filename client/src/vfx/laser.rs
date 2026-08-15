use bevy::prelude::*;

use crate::{actors::ActorMap, config::LaserVfxConfig, players::PlayerMap};
use common::{
    physics::CollisionWorld,
    protocol::{ActorId, PlayerId, SActorBeam},
};

// A live laser burst, anchored each frame to the interpolated actor and
// target entities — the server's beam tracks its target, so the wire carries
// only the start cue and the client derives both endpoints locally.
#[derive(Component)]
pub struct LaserBeam {
    pub actor: ActorId,
    pub target: PlayerId,
    pub remaining_secs: f32,
}

pub fn spawn_laser_beam(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    vfx: &LaserVfxConfig,
    msg: &SActorBeam,
) -> Entity {
    let brightness = vfx.emissive_brightness;
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
            },
            Mesh3d(meshes.add(Cylinder::new(vfx.beam_radius, 1.0))),
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
    collision_world: Option<Res<CollisionWorld>>,
    endpoints: Query<&Transform, Without<LaserBeam>>,
    mut beams: Query<(Entity, &mut LaserBeam, &mut Transform, &mut Visibility)>,
) {
    let delta = time.delta_secs();
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
        let target = target_transform.translation;
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
