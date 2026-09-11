use bevy::{
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};
use common::{
    config::CharacterPhysicsConfig,
    constants::CHARACTER_CONTACT_OFFSET,
    physics::{
        CharacterSupport, CharacterVerticalVelocity, CollisionWorld, GroundingDiagnostics, PortalSet,
        grounding_diagnostics,
    },
    protocol::{ActorMarker, FaceYaw, PlateState, PlayerId, Position},
};

use super::BoundsMode;
use crate::{
    cameras::MainCameraMarker,
    constants::{
        BOUNDS_AIRBORNE_COLOR, BOUNDS_CAPSULE_COLOR, BOUNDS_GROUNDED_COLOR, BOUNDS_HITBOX_COLOR, BOUNDS_LADDER_COLOR,
    },
    players::{CuboidShake, LocalMovementStep, PlayerMap, RemotePlayerMotion},
};

#[derive(Component)]
pub struct CharacterBounds {
    physics: CharacterPhysicsConfig,
}

#[derive(Component)]
pub struct BoundsShape(BoundsMode);

pub fn spawn_character_bounds(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    physics: CharacterPhysicsConfig,
) -> Entity {
    let root = commands
        .spawn((CharacterBounds { physics }, Transform::default(), Visibility::Inherited))
        .id();
    let body = physics.movement_collider;
    let capsule = meshes.add(Capsule3d::new(body.radius(), body.height - body.diameter));
    let hitbox = meshes.add(Cuboid::new(
        physics.hitbox.width,
        physics.hitbox.height,
        physics.hitbox.depth,
    ));
    for (mode, mesh, color) in [
        (BoundsMode::Grounding, capsule, BOUNDS_CAPSULE_COLOR),
        (BoundsMode::Hitbox, hitbox, BOUNDS_HITBOX_COLOR),
    ] {
        commands.spawn((
            BoundsShape(mode),
            ChildOf(root),
            Mesh3d(mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    root
}

pub fn character_bounds_sync_system(
    mode: Res<BoundsMode>,
    roots: Query<(&ChildOf, &CharacterBounds)>,
    actors: Query<(&FaceYaw, &Transform, Option<&CuboidShake>), Without<BoundsShape>>,
    mut shapes: Query<(&ChildOf, &BoundsShape, &mut Transform, &mut Visibility)>,
) {
    for (parent, marker, mut transform, mut visibility) in &mut shapes {
        let active = *mode != BoundsMode::Off && marker.0 == *mode;
        visibility.set_if_neq(if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if !active {
            continue;
        }
        let Ok((actor, bounds)) = roots.get(parent.parent()) else {
            continue;
        };
        let Ok((yaw, actor_transform, shake)) = actors.get(actor.parent()) else {
            continue;
        };
        let physics = bounds.physics;
        let origin = rendered_feet(actor_transform, shake);
        let (height, rotation) = match *mode {
            BoundsMode::Hitbox => (physics.hitbox.center_y_offset(), Quat::from_rotation_y(yaw.0)),
            _ => (
                physics.movement_collider.height / 2.0 + CHARACTER_CONTACT_OFFSET,
                Quat::IDENTITY,
            ),
        };
        *transform = bounds_transform(actor_transform, origin + Vec3::Y * height, rotation);
    }
}

fn rendered_feet(transform: &Transform, shake: Option<&CuboidShake>) -> Vec3 {
    transform.translation - shake.map_or(Vec3::ZERO, |shake| Vec3::new(shake.offset_x, 0.0, shake.offset_z))
}

fn bounds_transform(parent: &Transform, center: Vec3, rotation: Quat) -> Transform {
    Transform::from_translation(parent.compute_affine().inverse().transform_point3(center))
        .with_rotation(parent.rotation.inverse() * rotation)
        .with_scale(parent.scale.recip())
}

// Interpolated characters have no physics step to refresh their grounding diagnostics.
pub(crate) fn refresh_grounding_debug_system(
    mut commands: Commands,
    mode: Res<BoundsMode>,
    world: Res<CollisionWorld>,
    portals: Res<PortalSet>,
    plates: Res<PlateState>,
    players: Res<PlayerMap>,
    roots: Query<(&ChildOf, &CharacterBounds)>,
    characters: Query<
        (
            &Position,
            Option<&PlayerId>,
            &CharacterVerticalVelocity,
            Option<&CharacterSupport>,
        ),
        Or<(
            Without<GroundingDiagnostics>,
            With<RemotePlayerMotion>,
            With<ActorMarker>,
        )>,
    >,
) {
    if *mode != BoundsMode::Grounding {
        return;
    }
    for (parent, bounds) in &roots {
        let entity = parent.parent();
        let Ok((pos, player, motion, support)) = characters.get(entity) else {
            continue;
        };
        let keys = player
            .and_then(|id| players.get(id))
            .map_or(&[][..], |p| p.held_keys.as_slice());
        let passable = world.passable_barriers(keys, &plates.open_barriers);
        let excluded = if player.is_some() {
            portals.collision_exclusions(Vec3::from(*pos), bounds.physics)
        } else {
            Vec::new()
        };
        let mut ground = grounding_diagnostics(&world, pos, bounds.physics, &passable, &excluded);
        ground.supported &= motion.0 <= 0.0;
        commands.entity(entity).insert(ground);
        if support.is_none() {
            commands.entity(entity).insert(if ground.supported {
                CharacterSupport::Ground
            } else {
                CharacterSupport::Airborne
            });
        }
    }
}

pub fn grounding_debug_system(
    mode: Res<BoundsMode>,
    roots: Query<(&ChildOf, &CharacterBounds)>,
    query: Query<(
        &Position,
        &Transform,
        Option<&CuboidShake>,
        &GroundingDiagnostics,
        Option<&CharacterSupport>,
        Option<&LocalMovementStep>,
    )>,
    cameras: Query<&GlobalTransform, With<MainCameraMarker>>,
    mut gizmos: Gizmos,
) {
    if *mode != BoundsMode::Grounding {
        return;
    }
    let camera_rotation = cameras.single().ok().map(GlobalTransform::rotation);
    for (parent, bounds) in &roots {
        let Ok((pos, transform, shake, ground, support, step)) = query.get(parent.parent()) else {
            continue;
        };
        let origin = rendered_feet(transform, shake);
        // Diagnostics are sampled at physics ticks; draw them in the interpolated body's frame.
        let render_offset = origin - Vec3::from(*pos);
        // The local body keeps its support on its step; interpolated bodies carry the component.
        let support = step
            .map(|step| step.support)
            .or(support.copied())
            .unwrap_or(if ground.supported {
                CharacterSupport::Ground
            } else {
                CharacterSupport::Airborne
            });
        let (label, color) = match support {
            CharacterSupport::Ground => ("Grounded", BOUNDS_GROUNDED_COLOR),
            CharacterSupport::Airborne => ("Airborne", BOUNDS_AIRBORNE_COLOR),
            CharacterSupport::Ladder => ("Ladder", BOUNDS_LADDER_COLOR),
        };
        if let Some(rotation) = camera_rotation {
            let body = bounds.physics.movement_collider;
            let center = origin + Vec3::Y * (body.height / 2.0) + rotation * Vec3::X * (body.radius() + 0.15);
            gizmos.text(
                Isometry3d::new(center, rotation),
                label,
                0.12,
                Vec2::new(-0.5, 0.0),
                color,
            );
        }
        let probe_origin = ground.origin + render_offset;
        gizmos.line(probe_origin, probe_origin - Vec3::Y * ground.distance, color);
        if let Some(hit) = ground.hit {
            let contact = hit.contact + render_offset;
            gizmos.sphere(Isometry3d::from_translation(contact), 0.035, color);
            gizmos.arrow(contact, contact + hit.normal * 0.35, color);
        }
    }
}

#[cfg(test)]
#[path = "tests/inspection.rs"]
mod tests;
