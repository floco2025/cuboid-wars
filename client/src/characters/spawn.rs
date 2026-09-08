use bevy::{
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};
use common::{
    config::CharacterPhysicsConfig,
    constants::CHARACTER_CONTACT_OFFSET,
    physics::{
        CharacterSupport, CharacterVerticalVelocity, CollisionWorld, GroundingDiagnostics, PortalSet,
        grounding_diagnostics, passable_barrier_kinds,
    },
    protocol::{FaceYaw, PlateState, PlayerId, Position},
};

use super::BoundsMode;
use crate::{cameras::MainCameraMarker, players::PlayerMap};

#[derive(Component)]
pub struct CharacterBounds {
    physics: CharacterPhysicsConfig,
}

#[derive(Component)]
pub struct BoundsShapeMarker(BoundsMode);

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
    let capsule = meshes.add(Capsule3d::new(body.radius, body.height - body.radius * 2.0));
    let hitbox = meshes.add(Cuboid::new(
        physics.hitbox.width,
        physics.hitbox.height,
        physics.hitbox.depth,
    ));
    for (mode, mesh, color) in [
        (BoundsMode::Movement, capsule, Color::srgba(0.1, 0.8, 1.0, 0.18)),
        (BoundsMode::Hitbox, hitbox, Color::srgba(1.0, 0.2, 0.2, 0.18)),
    ] {
        commands.spawn((
            BoundsShapeMarker(mode),
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
    actors: Query<(&Position, &FaceYaw, &Transform), Without<BoundsShapeMarker>>,
    mut shapes: Query<(&ChildOf, &BoundsShapeMarker, &mut Transform, &mut Visibility)>,
) {
    for (parent, marker, mut transform, mut visibility) in &mut shapes {
        let active = match *mode {
            BoundsMode::Off => false,
            BoundsMode::Grounding => marker.0 == BoundsMode::Movement,
            _ => marker.0 == *mode,
        };
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
        let Ok((pos, yaw, actor_transform)) = actors.get(actor.parent()) else {
            continue;
        };
        let physics = bounds.physics;
        let (origin, height, rotation) = match *mode {
            BoundsMode::Hitbox => (
                Vec3::from(*pos),
                physics.hitbox.center_y_offset(),
                Quat::from_rotation_y(yaw.0),
            ),
            _ => (
                Vec3::from(*pos),
                physics.movement_collider.height / 2.0 + CHARACTER_CONTACT_OFFSET,
                Quat::IDENTITY,
            ),
        };
        *transform = bounds_transform(actor_transform, origin + Vec3::Y * height, rotation);
    }
}

fn bounds_transform(parent: &Transform, center: Vec3, rotation: Quat) -> Transform {
    Transform::from_translation(parent.compute_affine().inverse().transform_point3(center))
        .with_rotation(parent.rotation.inverse() * rotation)
        .with_scale(parent.scale.recip())
}

pub fn refresh_grounding_debug_system(
    mut commands: Commands,
    mode: Res<BoundsMode>,
    world: Res<CollisionWorld>,
    portals: Res<PortalSet>,
    plates: Res<PlateState>,
    players: Res<PlayerMap>,
    roots: Query<(&ChildOf, &CharacterBounds)>,
    actors: Query<(
        &Position,
        Option<&PlayerId>,
        &CharacterVerticalVelocity,
        Option<&GroundingDiagnostics>,
        Option<&CharacterSupport>,
    )>,
) {
    if *mode != BoundsMode::Grounding {
        return;
    }
    for (parent, bounds) in &roots {
        let entity = parent.parent();
        let Ok((pos, player, motion, cached, support)) = actors.get(entity) else {
            continue;
        };
        let origin = Vec3::from(*pos) + Vec3::Y * CHARACTER_CONTACT_OFFSET * 2.0;
        if cached.is_some_and(|g| g.origin.abs_diff_eq(origin, 1e-5)) {
            continue;
        }
        // Character blocking and portal hops can replace the motor's proposed position.
        let keys = player
            .and_then(|id| players.get(id))
            .map_or(&[][..], |p| p.held_keys.as_slice());
        let passable = passable_barrier_kinds(keys, &plates.open_barrier_kinds);
        let excluded = if player.is_some() {
            portals.collision_exclusions(Vec3::from(*pos), bounds.physics)
        } else {
            Vec::new()
        };
        let mut ground = grounding_diagnostics(&world, pos, bounds.physics, &passable, &excluded);
        ground.supported &= motion.0 <= 0.0;
        let support = if support == Some(&CharacterSupport::Ladder) && world.ladder_volume_at(pos).is_some() {
            CharacterSupport::Ladder
        } else if ground.supported {
            CharacterSupport::Ground
        } else {
            CharacterSupport::Airborne
        };
        commands.entity(entity).insert((ground, support));
    }
}

pub fn grounding_debug_system(
    mode: Res<BoundsMode>,
    roots: Query<(&ChildOf, &CharacterBounds)>,
    query: Query<(&Position, &GroundingDiagnostics, Option<&CharacterSupport>)>,
    cameras: Query<&GlobalTransform, With<MainCameraMarker>>,
    mut gizmos: Gizmos,
) {
    if *mode != BoundsMode::Grounding {
        return;
    }
    let camera_rotation = cameras.single().ok().map(GlobalTransform::rotation);
    for (parent, bounds) in &roots {
        let Ok((pos, ground, support)) = query.get(parent.parent()) else {
            continue;
        };
        let support = support.copied().unwrap_or(if ground.supported {
            CharacterSupport::Ground
        } else {
            CharacterSupport::Airborne
        });
        let (label, color) = match support {
            CharacterSupport::Ground => ("Grounded", Color::srgb(0.1, 1.0, 0.3)),
            CharacterSupport::Airborne => ("Airborne", Color::srgb(1.0, 0.6, 0.1)),
            CharacterSupport::Ladder => ("Ladder", Color::srgb(0.1, 0.8, 1.0)),
        };
        if let Some(rotation) = camera_rotation {
            let body = bounds.physics.movement_collider;
            let center = Vec3::from(*pos) + Vec3::Y * (body.height / 2.0) + rotation * Vec3::X * (body.radius + 0.15);
            gizmos.text(
                Isometry3d::new(center, rotation),
                label,
                0.12,
                Vec2::new(-0.5, 0.0),
                color,
            );
        }
        gizmos.line(ground.origin, ground.origin - Vec3::Y * ground.distance, color);
        if let Some(hit) = ground.hit {
            gizmos.sphere(Isometry3d::from_translation(hit.contact), 0.035, color);
            gizmos.arrow(hit.contact, hit.contact + hit.normal * 0.35, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::input_bounds_cycle_system;

    #[test]
    fn bounds_follow_physics_instead_of_render_interpolation_and_turning() {
        let parent = Transform::from_xyz(3.0, 1.0, 2.0).with_rotation(Quat::from_rotation_y(0.8));
        let center = Vec3::new(3.1, 1.9, 2.2);
        for rotation in [Quat::IDENTITY, Quat::from_rotation_y(-1.3)] {
            let child = bounds_transform(&parent, center, rotation);
            let world = parent.mul_transform(child);
            assert!(world.translation.abs_diff_eq(center, 1e-5));
            assert!(world.rotation.abs_diff_eq(rotation, 1e-5));
        }
    }

    #[test]
    fn keyboard_cycles_all_modes_for_existing_and_later_shapes() {
        let mut app = App::new();
        app.init_resource::<BoundsMode>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(
                Update,
                (input_bounds_cycle_system, character_bounds_sync_system).chain(),
            );
        let root = app.world_mut().spawn_empty().id();
        let first = app
            .world_mut()
            .spawn((
                ChildOf(root),
                BoundsShapeMarker(BoundsMode::Movement),
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        assert_eq!(*app.world().resource::<BoundsMode>(), BoundsMode::Off);
        for expected in [
            BoundsMode::Movement,
            BoundsMode::Hitbox,
            BoundsMode::Grounding,
            BoundsMode::Off,
        ] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::KeyB);
            app.update();
            assert_eq!(*app.world().resource::<BoundsMode>(), expected);
            assert_eq!(
                *app.world().get::<Visibility>(first).expect("bounds visibility missing"),
                if matches!(expected, BoundsMode::Movement | BoundsMode::Grounding) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                }
            );
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
            let later = app
                .world_mut()
                .spawn((
                    ChildOf(root),
                    BoundsShapeMarker(if expected == BoundsMode::Grounding {
                        BoundsMode::Movement
                    } else {
                        expected
                    }),
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .id();
            app.update();
            assert_eq!(
                *app.world().get::<Visibility>(later).expect("bounds visibility missing"),
                if expected == BoundsMode::Off {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                }
            );
        }
    }
}
